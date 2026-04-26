//! Build script:
//!
//! 1. **runtime.json**: regenera desde `classifier/.env`.
//! 2. **Modelo**: hardlink-ea cada archivo de `classifier/onnx_model/` →
//!    `app/src-tauri/resources/onnx_model/*`.
//!
//! Para iOS, scripts/setup.sh prepara libonnxruntime.a en .ort_link/<target>/
//! antes de cargo build (build script override en .cargo/config.toml hace que
//! ort-sys lo encuentre estáticamente). Si no se corrió setup.sh, el build de
//! iOS falla con un mensaje claro.

use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    if let Err(e) = generate_runtime_json(&manifest) {
        println!("cargo:warning=runtime.json no se generó: {e}");
    }

    sync_model_resources(&manifest);

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "ios" {
        warn_if_setup_missing(&manifest);
    }

    tauri_build::build();
}

fn warn_if_setup_missing(manifest: &Path) {
    let target = std::env::var("TARGET").unwrap_or_default();
    let lib = manifest.join(format!(".ort_link/{target}/libonnxruntime.a"));
    if !lib.exists() {
        println!(
            "cargo:warning=falta {}. Corre `bash scripts/setup.sh` desde la raíz del repo.",
            lib.display()
        );
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

    let runtime = json!({
        "model_id": require("NLI_MODEL")?,
        "category_keys": keys_val,
        "hypotheses": hypotheses,
        "lexical": lexical,
        "neutral_hypothesis": require("NEUTRAL_HYPOTHESIS")?,
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
    std::fs::write(&out_path, serde_json::to_string_pretty(&runtime).unwrap())
        .map_err(|e| format!("write {}: {e}", out_path.display()))?;

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

