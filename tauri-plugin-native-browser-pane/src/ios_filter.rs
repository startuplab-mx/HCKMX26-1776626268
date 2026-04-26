//! FFI Rust → Swift para que `FilterMessageHandler` (Swift) delegue el
//! filtrado al classifier zero-shot multi-hipótesis.
//!
//! El classifier vive como `OnceLock<Arc<Classifier>>` en classifier,
//! seteado por la app en su setup hook. Aquí solo lo leemos.
//!
//! Critical: TODO el body va dentro de `catch_unwind` para que panics
//! no crucen el boundary FFI (UB → app abort). Si algo revienta, log y
//! passthrough.

use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::panic::{catch_unwind, AssertUnwindSafe};

use classifier::{
    apply_filter_batch, ImageDecision, SHARED_CLASSIFIER, SHARED_IMAGE_CLASSIFIER,
};
use swift_rs::{SRData, SRString};

/// Limite de seguridad: textos individuales más largos se truncan.
const MAX_TEXT_LEN: usize = 4000;
/// Tamaño de chunk para procesar el batch en porciones. Más chico = más
/// rápido el primer resultado visible al DOM (UX percibida), aunque la
/// latencia total agregada es similar (mDeBERTa CPU ~50-100 ms por par).
/// 12 textos × ~13 hipótesis = ~156 pares por session.run() ≈ 1-2 s en
/// device — punto dulce para que el JS pueda hacer streaming visible cada
/// llamada FFI.
const CHUNK_SIZE: usize = 12;

#[no_mangle]
pub extern "C" fn classifier_filter_texts(
    bundle_path: SRString,
    texts_json: SRString,
) -> SRString {
    // CRITICAL: los SRString llegan ya owned por Swift ARC. Si dejamos que
    // Rust llame Drop al fin del scope, swift-rs hace `swift_release` y luego
    // Swift también lo hace por ARC → double-release → SIGSEGV en objc_release.
    // ManuallyDrop suprime el Drop de Rust; Swift libera correctamente solo.
    let bundle_path = ManuallyDrop::new(bundle_path);
    let texts_json = ManuallyDrop::new(texts_json);
    let _ = &*bundle_path; // unused but document we received it
    let json_str = texts_json.as_str();

    let result = catch_unwind(AssertUnwindSafe(|| -> String {
        let mut inputs: Vec<String> = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[plugin classifier] JSON parse error: {e}");
                return "[]".to_string();
            }
        };

        for t in &mut inputs {
            if t.len() > MAX_TEXT_LEN {
                t.truncate(MAX_TEXT_LEN);
            }
        }

        // Dedup intra-batch antes de cruzar al clasificador. SPAs como
        // Telegram repiten "@username", botones, fechas, etc. en muchos nodos
        // del DOM dentro de un mismo scan. Clasificar solo el conjunto único
        // y reconstituir el array de salida en el orden original.
        let mut unique: Vec<String> = Vec::with_capacity(inputs.len());
        let mut map: Vec<usize> = Vec::with_capacity(inputs.len());
        let mut seen: HashMap<String, usize> = HashMap::with_capacity(inputs.len());
        for t in inputs.iter() {
            if let Some(&i) = seen.get(t) {
                map.push(i);
            } else {
                let i = unique.len();
                seen.insert(t.clone(), i);
                unique.push(t.clone());
                map.push(i);
            }
        }

        eprintln!(
            "[plugin classifier] processing {} texts ({} unique) in chunks of {}",
            inputs.len(),
            unique.len(),
            CHUNK_SIZE
        );

        let outputs = match SHARED_CLASSIFIER.get() {
            Some(classifier) => {
                let mut unique_out: Vec<String> = Vec::with_capacity(unique.len());
                for chunk in unique.chunks(CHUNK_SIZE) {
                    let out = apply_filter_batch(classifier, chunk);
                    unique_out.extend(out);
                }
                let expanded: Vec<String> =
                    map.iter().map(|&i| unique_out[i].clone()).collect();
                eprintln!(
                    "[plugin classifier] done: {}/{} outputs (classified {} unique)",
                    expanded.len(),
                    inputs.len(),
                    unique.len()
                );
                expanded
            }
            None => {
                eprintln!("[plugin classifier] SHARED no inicializado, passthrough");
                inputs
            }
        };

        serde_json::to_string(&outputs).unwrap_or_else(|_| "[]".to_string())
    }));

    match result {
        Ok(json) => SRString::from(json.as_str()),
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "<unknown payload>".to_string()
            };
            eprintln!("[plugin classifier] PANIC caught at FFI boundary: {msg}");
            // Devolver array vacío evita crashes en JS deserializing.
            SRString::from("[]")
        }
    }
}

/// Análogo a `classifier_filter_texts` pero para imágenes. El handler Swift
/// (FilterMessageHandler.filterImage) descarga los bytes de la imagen, llama
/// aquí, y según el veredicto blurea o devuelve la URL original tal cual.
///
/// Veredictos posibles (SRString):
/// - `"allow"`  → MobileCLIP la clasificó como benigna; no aplicar blur.
/// - `"block"` → cae en una categoría de riesgo; Swift aplica CIGaussianBlur.
/// - `"none"`  → el modelo no está cargado o decode falló; Swift mantiene
///                el comportamiento conservador (blur). Mismo contrato que
///                `filter_image_bytes` en desktop (lib.rs:441-447).
///
/// Mismo patrón de seguridad que el path de textos: `ManuallyDrop` para
/// evitar double-release del SR type owned por Swift ARC, y `catch_unwind`
/// para que un panic en `image::load_from_memory` o en ORT no cruce el
/// boundary FFI (UB → SIGABRT).
#[no_mangle]
pub extern "C" fn classifier_classify_image_bytes(bytes: SRData) -> SRString {
    let bytes = ManuallyDrop::new(bytes);

    let result = catch_unwind(AssertUnwindSafe(|| -> &'static str {
        let classifier = match SHARED_IMAGE_CLASSIFIER.get() {
            Some(c) => c,
            None => {
                eprintln!("[plugin image_classifier] SHARED no inicializado, fail-closed");
                return "none";
            }
        };

        match classifier.classify(bytes.as_slice()) {
            Ok(ImageDecision::Allow) => "allow",
            Ok(ImageDecision::Block) => "block",
            Err(e) => {
                eprintln!("[plugin image_classifier] classify error: {e}");
                "none"
            }
        }
    }));

    match result {
        Ok(verdict) => SRString::from(verdict),
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "<unknown payload>".to_string()
            };
            eprintln!(
                "[plugin image_classifier] PANIC caught at FFI boundary: {msg}"
            );
            SRString::from("none")
        }
    }
}
