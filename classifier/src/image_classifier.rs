//! Clasificador de imágenes basado en MobileCLIP-S1 (image encoder ONNX) +
//! pool de "text anchors" pre-computados. Decide si una imagen entra a la
//! categoría riesgo o segura comparando similitudes coseno contra los anchors.
//!
//! Esto es una implementación zero-shot V-NLI: en vez de entrenar un
//! clasificador binario, comparamos el embedding de la imagen contra
//! embeddings de frases descriptivas ("a photo of weapons…", "a photo of a
//! pet…"). Las similitudes pasan por softmax con temperatura logit_scale
//! para producir probabilidades.
//!
//! Decision rule (igual a la del notebook del compañero):
//!   bloquear = (best_risk > best_safe + margin) AND (best_risk > threshold)
//!
//! El layout de los anchors (cuántos son riesgo, cuántos seguros) viene
//! hard-coded por convención: las primeras `n_risk` filas del .npy son las
//! categorías de riesgo, el resto son las seguras.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use image::imageops::FilterType;
use ndarray::{Array2, Array4};
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Tensor,
};

/// Tamaño esperado por el ONNX exportado. MobileCLIP-S1 fue entrenado a
/// 256×256 (mobileclip/configs/mobileclip_s1.json: image_cfg.image_size).
/// `nsfw-py/src/export.py` debe usar el mismo valor — si difiere, ORT
/// rechazará el input shape y la inferencia caerá al branch de error
/// (fail-closed = blur). Antes era 224 — desalineado con el régimen
/// entrenado, generaba false-positive blocks.
const INPUT_SIZE: u32 = 256;
/// Número de anchors de riesgo. Los primeros 7 según el notebook:
/// drogas, desnudez, armas, narco, violencia, muerte, gore.
const N_RISK_ANCHORS: usize = 7;
/// Logit scale (temperatura inversa) — coincide con el `100.0 *` en el
/// notebook al hacer `(image @ text.T).softmax`.
const LOGIT_SCALE: f32 = 100.0;
/// Umbrales de decisión, calibrados sobre el dataset de armas + nsfw del
/// compañero (~73% accuracy con estos valores).
const RISK_THRESHOLD: f32 = 0.55;
const SAFETY_MARGIN: f32 = 0.10;

/// Decisión simple: si la imagen debe censurarse o no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageDecision {
    /// La imagen es benigna — devolverla tal cual.
    Allow,
    /// La imagen disparó una categoría de riesgo — aplicar blur antes de
    /// devolverla al WebView.
    Block,
}

pub struct ImageClassifier {
    session: Mutex<Session>,
    /// Anchors en filas, shape = (n_anchors, embed_dim). Ya están normalizados
    /// (norma L2 = 1) según el notebook que los exportó.
    anchors: Array2<f32>,
    n_risk: usize,
}

impl ImageClassifier {
    pub fn new(model_path: &Path, anchors_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("ort builder: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("ort opt level: {e}"))?
            .with_intra_threads(2)
            .map_err(|e| anyhow!("ort threads: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("ort commit: {e}"))?;

        let bytes = std::fs::read(anchors_path)
            .with_context(|| format!("leyendo {}", anchors_path.display()))?;
        let anchors = parse_npy_f32(&bytes)
            .with_context(|| format!("parseando {}", anchors_path.display()))?;

        if anchors.shape()[0] <= N_RISK_ANCHORS {
            return Err(anyhow!(
                "anchors npy debe tener >{} filas, tiene {}",
                N_RISK_ANCHORS,
                anchors.shape()[0]
            ));
        }

        Ok(Self {
            session: Mutex::new(session),
            anchors,
            n_risk: N_RISK_ANCHORS,
        })
    }

    /// Decide si los `bytes` corresponden a una imagen que debe censurarse.
    /// Errores de decodificación se propagan; el caller suele tratarlos como
    /// "fail-closed" (blur por seguridad).
    pub fn classify(&self, bytes: &[u8]) -> Result<ImageDecision> {
        let img = image::load_from_memory(bytes)
            .map_err(|e| anyhow!("decode imagen: {e}"))?;
        let img_w = img.width();
        let img_h = img.height();
        let tensor = preprocess(&img);

        let mut session = self
            .session
            .lock()
            .map_err(|e| anyhow!("session lock: {e}"))?;
        let outputs = session
            .run(ort::inputs![
                "image" => Tensor::from_array(tensor).map_err(|e| anyhow!("ort tensor: {e}"))?,
            ])
            .map_err(|e| anyhow!("ort run: {e}"))?;

        let (_shape, feats_flat) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("ort extract: {e}"))?;
        let embed_dim = self.anchors.shape()[1];
        if feats_flat.len() != embed_dim {
            return Err(anyhow!(
                "image feature dim {} ≠ anchors dim {}",
                feats_flat.len(),
                embed_dim
            ));
        }

        // L2-normalize la feature de la imagen — los anchors ya están
        // normalizados (lo hizo el .py al exportarlos).
        let mut img_feat: Vec<f32> = feats_flat.to_vec();
        let norm: f32 = img_feat.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut img_feat {
                *v /= norm;
            }
        }

        // Similitud coseno = dot(feature, anchor_i) por cada fila.
        let mut sims = Vec::with_capacity(self.anchors.shape()[0]);
        for row in self.anchors.rows() {
            let mut s = 0.0f32;
            for (a, b) in row.iter().zip(img_feat.iter()) {
                s += a * b;
            }
            sims.push(s * LOGIT_SCALE);
        }

        // Softmax estable.
        let max = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let mut probs: Vec<f32> = sims.iter().map(|&s| (s - max).exp()).collect();
        let sum: f32 = probs.iter().sum();
        if sum > 0.0 {
            for p in &mut probs {
                *p /= sum;
            }
        }

        let best_risk = probs[..self.n_risk]
            .iter()
            .cloned()
            .fold(0.0f32, f32::max);
        let best_safe = probs[self.n_risk..]
            .iter()
            .cloned()
            .fold(0.0f32, f32::max);

        let block = best_risk > best_safe + SAFETY_MARGIN && best_risk > RISK_THRESHOLD;
        let decision = if block {
            ImageDecision::Block
        } else {
            ImageDecision::Allow
        };
        // Diagnóstico para builds dev: si el usuario reporta que todo se
        // bluea, este log responde rápido si (a) la inferencia se está
        // ejecutando, (b) qué scores produce y (c) por qué cae del lado
        // block/allow. En release queda fuera por costo.
        #[cfg(debug_assertions)]
        eprintln!(
            "[image_classifier] {}x{} risk={:.3} safe={:.3} -> {:?}",
            img_w, img_h, best_risk, best_safe, decision
        );
        Ok(decision)
    }
}

/// Resize → center-crop → NCHW float32 [0,1]. Sin Normalize: MobileCLIP usa
/// `Compose([Resize(s), CenterCrop(s), ToTensor()])` (verificado en
/// apple/ml-mobileclip/mobileclip/__init__.py).
fn preprocess(img: &image::DynamicImage) -> Array4<f32> {
    let (w, h) = (img.width(), img.height());
    // Resize a shorter-side = INPUT_SIZE, preservando aspect ratio.
    let (rw, rh) = if w < h {
        (INPUT_SIZE, (h as f64 * INPUT_SIZE as f64 / w as f64).round() as u32)
    } else {
        ((w as f64 * INPUT_SIZE as f64 / h as f64).round() as u32, INPUT_SIZE)
    };
    let resized = img
        .resize_exact(rw.max(INPUT_SIZE), rh.max(INPUT_SIZE), FilterType::Triangle)
        .to_rgb8();

    // Center crop a INPUT_SIZE × INPUT_SIZE.
    let (rw, rh) = (resized.width(), resized.height());
    let x0 = (rw - INPUT_SIZE) / 2;
    let y0 = (rh - INPUT_SIZE) / 2;
    let cropped = image::imageops::crop_imm(&resized, x0, y0, INPUT_SIZE, INPUT_SIZE).to_image();

    // Pixels HWC u8 → CHW float32 / 255.
    let s = INPUT_SIZE as usize;
    let mut tensor = Array4::<f32>::zeros((1, 3, s, s));
    for (x, y, p) in cropped.enumerate_pixels() {
        let xs = x as usize;
        let ys = y as usize;
        tensor[[0, 0, ys, xs]] = p[0] as f32 / 255.0;
        tensor[[0, 1, ys, xs]] = p[1] as f32 / 255.0;
        tensor[[0, 2, ys, xs]] = p[2] as f32 / 255.0;
    }
    tensor
}

/// Parser mínimo de .npy v1/v2 para float32 little-endian, 2-D, C-order.
/// Cubre el caso del archivo del compañero — no soporta otros dtypes.
fn parse_npy_f32(bytes: &[u8]) -> Result<Array2<f32>> {
    if bytes.len() < 10 || &bytes[..6] != b"\x93NUMPY" {
        return Err(anyhow!("npy: magic header inválido"));
    }
    let major = bytes[6];
    let header_offset = match major {
        1 => {
            let len = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
            (10, len)
        }
        2 | 3 => {
            let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
            (12, len)
        }
        v => return Err(anyhow!("npy: versión {v} no soportada")),
    };
    let (start, len) = header_offset;
    if start + len > bytes.len() {
        return Err(anyhow!("npy: header trunco"));
    }
    let header = std::str::from_utf8(&bytes[start..start + len])
        .map_err(|e| anyhow!("npy header utf8: {e}"))?;

    if !header.contains("'descr': '<f4'") && !header.contains("\"descr\": \"<f4\"") {
        return Err(anyhow!("npy: solo se soporta float32 LE; header={header}"));
    }
    if header.contains("'fortran_order': True") || header.contains("\"fortran_order\": true") {
        return Err(anyhow!("npy: fortran_order=True no soportado"));
    }

    // Shape: extrae los dos enteros entre paréntesis después de 'shape':
    let shape_start = header
        .find("'shape':")
        .or_else(|| header.find("\"shape\":"))
        .ok_or_else(|| anyhow!("npy header sin shape"))?;
    let after = &header[shape_start..];
    let lp = after.find('(').ok_or_else(|| anyhow!("npy shape sin ("))?;
    let rp = after.find(')').ok_or_else(|| anyhow!("npy shape sin )"))?;
    let dims_str = &after[lp + 1..rp];
    let dims: Vec<usize> = dims_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<usize>().map_err(|e| anyhow!("npy shape parse: {e}")))
        .collect::<Result<_>>()?;
    if dims.len() != 2 {
        return Err(anyhow!("npy: solo soporta arrays 2-D, got {:?}", dims));
    }

    let data_start = start + len;
    let elems = dims[0] * dims[1];
    let needed = elems * 4;
    if data_start + needed > bytes.len() {
        return Err(anyhow!("npy: datos truncos"));
    }
    let mut floats = Vec::with_capacity(elems);
    for i in 0..elems {
        let off = data_start + i * 4;
        floats.push(f32::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
        ]));
    }
    Ok(Array2::from_shape_vec((dims[0], dims[1]), floats)?)
}
