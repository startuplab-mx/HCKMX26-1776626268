//! Build script:
//!
//! 1. **runtime.json**: regenera desde `classifier/.env`.
//! 2. **Modelo**: hardlink-ea cada archivo de `classifier/onnx_model/` →
//!    `app/src-tauri/resources/onnx_model/*`.
//! 3. **iOS link**: descarga onnxruntime.xcframework (~30 MB) si falta,
//!    extrae el slice del target con `lipo -thin`, y emite los
//!    `cargo:rustc-link-*` para enlazar libonnxruntime.a estáticamente.
//!    Reemplaza al antiguo `scripts/setup.sh` + override en
//!    `.cargo/config.toml`. Funciona porque `classifier-core` activa la
//!    feature `alternative-backend` de `ort` en iOS (deshabilita el linking
//!    automático de `ort-sys`), y `lib.rs` llama a `init_ort_api()` al startup.

use std::path::{Path, PathBuf};

const ORT_VERSION: &str = "1.22.0";

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    if let Err(e) = generate_runtime_json(&manifest) {
        println!("cargo:warning=runtime.json no se generó: {e}");
    }

    sync_model_resources(&manifest);

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "ios" {
        if let Err(e) = setup_ort_ios(&manifest) {
            panic!("setup ort iOS falló: {e}");
        }
    }

    tauri_build::build();
}

// ----------------------------------------------------------------------------
// 3. iOS: ort static linking (ex-scripts/setup.sh)
// ----------------------------------------------------------------------------

fn setup_ort_ios(manifest: &Path) -> Result<(), String> {
    let target = std::env::var("TARGET").map_err(|e| e.to_string())?;

    // Mapping target → (xcframework slice, lipo arch name). El xcframework de
    // Microsoft trae device (ios-arm64) y simulator (ios-arm64_x86_64-simulator).
    // Sólo soportamos arm64 — x86_64 simulator quedó deprecado. Notar que
    // `lipo` espera "arm64", no "aarch64" (el nombre rust de la arquitectura).
    let (slice, arch) = match target.as_str() {
        "aarch64-apple-ios" => ("ios-arm64", "arm64"),
        "aarch64-apple-ios-sim" => ("ios-arm64_x86_64-simulator", "arm64"),
        other => return Err(format!("target iOS no soportado: {other}")),
    };

    // Vendor location compartida con el Swift package del plugin nativo.
    let xcfw_root = manifest
        .join("../../tauri-plugin-native-browser-pane/ios/vendor/onnxruntime.xcframework");

    if !xcfw_root.exists() {
        download_xcframework(&xcfw_root)?;
    }

    let src = xcfw_root
        .join(slice)
        .join("onnxruntime.framework/onnxruntime");
    if !src.exists() {
        return Err(format!("slice no encontrado en xcframework: {}", src.display()));
    }

    let dst_dir = manifest.join(format!(".ort_link/{target}"));
    std::fs::create_dir_all(&dst_dir).map_err(|e| format!("mkdir {}: {e}", dst_dir.display()))?;
    let dst = dst_dir.join("libonnxruntime.a");

    if needs_relipo(&src, &dst) {
        let _ = std::fs::remove_file(&dst);
        let status = std::process::Command::new("lipo")
            .args(["-thin", arch, "-output"])
            .arg(&dst)
            .arg(&src)
            .status()
            .map_err(|e| format!("ejecutar lipo: {e}"))?;
        if !status.success() {
            return Err(format!("lipo falló para {target} ({arch})"));
        }
    }

    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed={}", dst.display());

    // CARGO_MANIFEST_DIR es absoluto, así que dst_dir también — esquivamos
    // la limitación de `.cargo/config.toml` que no expande relativos a
    // través de build script overrides.
    println!("cargo:rustc-link-search=native={}", dst_dir.display());
    println!("cargo:rustc-link-lib=static=onnxruntime");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rustc-link-lib=framework=CoreML");

    Ok(())
}

fn download_xcframework(dst: &Path) -> Result<(), String> {
    let url = format!(
        "https://download.onnxruntime.ai/pod-archive-onnxruntime-c-{ORT_VERSION}.zip"
    );
    println!("cargo:warning=descargando onnxruntime {ORT_VERSION} (~30 MB)...");

    let parent = dst
        .parent()
        .ok_or_else(|| format!("xcfw destino sin parent: {}", dst.display()))?;
    std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;

    let tmp_dir = parent.join(".onnxruntime-download.tmp");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("mkdir tmp: {e}"))?;
    let zip_path = tmp_dir.join("ort.zip");

    let curl = std::process::Command::new("curl")
        .arg("-fsSLo")
        .arg(&zip_path)
        .arg(&url)
        .status()
        .map_err(|e| format!("ejecutar curl: {e}"))?;
    if !curl.success() {
        return Err(format!("curl falló descargando {url}"));
    }

    let unzip = std::process::Command::new("unzip")
        .arg("-q")
        .arg(&zip_path)
        .arg("-d")
        .arg(&tmp_dir)
        .status()
        .map_err(|e| format!("ejecutar unzip: {e}"))?;
    if !unzip.success() {
        return Err("unzip falló".into());
    }

    let extracted = tmp_dir.join("onnxruntime.xcframework");
    if !extracted.exists() {
        return Err(format!(
            "xcframework no encontrado tras unzip en {}",
            extracted.display()
        ));
    }
    std::fs::rename(&extracted, dst)
        .map_err(|e| format!("rename {} → {}: {e}", extracted.display(), dst.display()))?;
    let _ = std::fs::remove_dir_all(&tmp_dir);
    Ok(())
}

fn needs_relipo(src: &Path, dst: &Path) -> bool {
    if !dst.exists() {
        return true;
    }
    let (Ok(s), Ok(d)) = (std::fs::metadata(src), std::fs::metadata(dst)) else {
        return true;
    };
    match (s.modified(), d.modified()) {
        (Ok(sm), Ok(dm)) => sm > dm,
        _ => true,
    }
}

// ----------------------------------------------------------------------------
// 1. runtime.json
// ----------------------------------------------------------------------------

fn generate_runtime_json(manifest: &Path) -> Result<(), String> {
    use serde_json::{json, Value};

    let env_path = manifest.join("../../classifier/.env");
    let env_path = env_path
        .canonicalize()
        .map_err(|_| format!("classifier/.env no encontrado en {}", env_path.display()))?;

    println!("cargo:rerun-if-changed={}", env_path.display());

    let env = parse_dotenv_simple(&env_path)?;

    let require = |key: &str| -> Result<String, String> {
        env.get(key).cloned().ok_or_else(|| format!("falta {key} en .env"))
    };
    let parse_json = |s: &str, ctx: &str| -> Result<Value, String> {
        serde_json::from_str(s).map_err(|e| format!("parse {ctx}: {e}"))
    };

    let keys_val: Value = parse_json(&require("CATEGORY_KEYS")?, "CATEGORY_KEYS")?;
    let keys: Vec<String> = serde_json::from_value(keys_val.clone())
        .map_err(|e| format!("CATEGORY_KEYS no es array de strings: {e}"))?;

    let mut hypotheses = serde_json::Map::new();
    let mut lexical = serde_json::Map::new();
    for k in &keys {
        let hyp_key = format!("HYPOTHESES_{}", k.to_uppercase());
        let lex_key = format!("LEXICAL_{}", k.to_uppercase());
        hypotheses.insert(k.clone(), parse_json(&require(&hyp_key)?, &hyp_key)?);
        lexical.insert(k.clone(), parse_json(&require(&lex_key)?, &lex_key)?);
    }

    // Pool de neutrales: preferimos NEUTRAL_HYPOTHESES (lista JSON). Si sólo
    // está el NEUTRAL_HYPOTHESIS legacy (string), lo envolvemos en lista para
    // mantener compat con .env viejos.
    let neutral_hypotheses_val: Value = match env.get("NEUTRAL_HYPOTHESES") {
        Some(s) => parse_json(s, "NEUTRAL_HYPOTHESES")?,
        None => {
            let single = require("NEUTRAL_HYPOTHESIS")?;
            json!([single])
        }
    };

    let runtime = json!({
        "model_id": require("NLI_MODEL")?,
        "category_keys": keys_val,
        "hypotheses": hypotheses,
        "lexical": lexical,
        "neutral_hypotheses": neutral_hypotheses_val,
        "thresholds": parse_json(&require("THRESHOLDS")?, "THRESHOLDS")?,
        "test_cases": parse_json(&require("TEST_CASES")?, "TEST_CASES")?,
        "context_test_cases": parse_json(
            env.get("CONTEXT_TEST_CASES").map(|s| s.as_str()).unwrap_or("[]"),
            "CONTEXT_TEST_CASES",
        )?,
        "lexical_shortcut_score": 0.95,
        "lexical_boost_floor": 0.70,
        "max_context": 4,
    });

    let out_path = manifest.join("resources/runtime.json");
    let _ = std::fs::create_dir_all(out_path.parent().unwrap());
    let new_contents = serde_json::to_string_pretty(&runtime).unwrap();
    // Sólo escribir si cambió — si tocamos mtime sin necesidad, el watcher
    // de Tauri dev detecta el cambio en resources/runtime.json y vuelve a
    // disparar la build, creando un loop infinito.
    let needs_write = match std::fs::read_to_string(&out_path) {
        Ok(existing) => existing != new_contents,
        Err(_) => true,
    };
    if needs_write {
        std::fs::write(&out_path, new_contents)
            .map_err(|e| format!("write {}: {e}", out_path.display()))?;
    }

    Ok(())
}

/// Parser dotenv minimal: cada linea `KEY=VALUE`. El valor es tomado como
/// literal hasta fin de linea (sin procesar quotes ni escapes), porque
/// nuestros valores son JSON inline.
fn parse_dotenv_simple(
    path: &Path,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut map = std::collections::BTreeMap::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            let val = line[eq + 1..].to_string();
            map.insert(key, val);
        }
    }
    Ok(map)
}

// ----------------------------------------------------------------------------
// 2. Hardlink del modelo a resources/
// ----------------------------------------------------------------------------

fn sync_model_resources(manifest: &Path) {
    let src_dir = manifest.join("../../classifier/onnx_model");
    let dst_dir = manifest.join("resources/onnx_model");

    let _ = std::fs::create_dir_all(&dst_dir);

    if !src_dir.exists() {
        let placeholder = dst_dir.join(".no-model");
        if !placeholder.exists() {
            let _ = std::fs::write(
                &placeholder,
                "# El modelo no se ha exportado todavia.\n\
                 # Corre: cd classifier && uv run --extra export python src/export.py\n"
                    .as_bytes(),
            );
        }
        println!(
            "cargo:warning=classifier/onnx_model/ no existe — corriendo en passthrough. \
             Corre: cd classifier && uv run --extra export python src/export.py"
        );
        return;
    }

    println!("cargo:rerun-if-changed={}", src_dir.display());

    let entries = match std::fs::read_dir(&src_dir) {
        Ok(e) => e,
        Err(e) => {
            println!("cargo:warning=read_dir({}) falló: {e}", src_dir.display());
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let dst = dst_dir.join(entry.file_name());

        if dst.exists() {
            if same_inode(&path, &dst).unwrap_or(false) {
                continue;
            }
            let _ = std::fs::remove_file(&dst);
        }

        if std::fs::hard_link(&path, &dst).is_err() {
            if let Err(e) = std::fs::copy(&path, &dst) {
                println!(
                    "cargo:warning=no se pudo linkear ni copiar {} → {}: {e}",
                    path.display(),
                    dst.display()
                );
            }
        }
    }

    let placeholder = dst_dir.join(".no-model");
    if placeholder.exists() {
        let _ = std::fs::remove_file(&placeholder);
    }
}

#[cfg(unix)]
fn same_inode(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let ma = std::fs::metadata(a)?;
    let mb = std::fs::metadata(b)?;
    Ok(ma.dev() == mb.dev() && ma.ino() == mb.ino())
}

#[cfg(not(unix))]
fn same_inode(_a: &Path, _b: &Path) -> std::io::Result<bool> {
    Ok(false)
}

