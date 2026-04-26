//! Hornea `SHIELD_AUTH_TOKEN` en el binario al compilar.
//!
//! Motivación: cuando la `app` corre como bundle (iOS/macOS), su cwd es el
//! `.app`, no la raíz del workspace, así que `dotenvy::dotenv()` no
//! encuentra el `.env`. Embebemos el token en el binario para que
//! `common::auth_token()` siempre tenga un fallback consistente con el
//! resto de los procesos del workspace (server, dashboard).
//!
//! Precedencia: shell env > .env de la raíz al build > vacío (panic en
//! runtime). El runtime sigue prefiriendo `std::env::var` para permitir
//! overrides en CI / prod.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    let mut env_path = None;
    for ancestor in manifest_dir.ancestors() {
        let candidate = ancestor.join(".env");
        if candidate.is_file() {
            env_path = Some(candidate);
            break;
        }
    }

    let from_file = env_path.as_ref().and_then(|path| {
        // Si .env cambia, recompila para re-hornear el token.
        println!("cargo:rerun-if-changed={}", path.display());
        std::fs::read_to_string(path).ok()
    });

    let baked = std::env::var("SHIELD_AUTH_TOKEN").ok().unwrap_or_else(|| {
        from_file
            .as_deref()
            .and_then(|c| extract_var(c, "SHIELD_AUTH_TOKEN"))
            .unwrap_or_default()
    });

    println!("cargo:rustc-env=SHIELD_AUTH_TOKEN_BUILD={baked}");
    println!("cargo:rerun-if-env-changed=SHIELD_AUTH_TOKEN");
}

/// Parser dotenv minimal — sólo soporta `KEY=VALUE` literal por línea (sin
/// quotes ni escapes), suficiente para nuestro `.env` con tokens hex.
fn extract_var(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(eq) = line.find('=') {
            if line[..eq].trim() == key {
                return Some(line[eq + 1..].to_string());
            }
        }
    }
    None
}
