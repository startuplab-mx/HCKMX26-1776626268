//! Tauri backend del dashboard. Proxy thin sobre el server Actix:
//! mantiene el bearer token en Rust (no lo expone al webview) y le da
//! al frontend dos commands tipados — `fetch_events` y `clear_events`.
//!
//! El token se lee de `SHIELD_AUTH_TOKEN` (env var o `.env` cargado por
//! `dotenvy`); nunca está hardcoded.

use common::{auth_token, server_url, FilterEvent};

/// Lista los eventos en el server. `since` opcional → polling incremental.
#[tauri::command]
async fn fetch_events(since: Option<i64>) -> Result<Vec<FilterEvent>, String> {
    let mut url = format!("{}/events", server_url());
    if let Some(s) = since {
        url.push_str(&format!("?since={s}"));
    }
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .bearer_auth(auth_token())
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("server respondió {}", resp.status()));
    }
    resp.json::<Vec<FilterEvent>>().await.map_err(|e| e.to_string())
}

/// Borra todos los eventos del server.
#[tauri::command]
async fn clear_events() -> Result<(), String> {
    let url = format!("{}/events", server_url());
    let client = reqwest::Client::new();
    let resp = client
        .delete(&url)
        .bearer_auth(auth_token())
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("server respondió {}", resp.status()));
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Carga `.env` (raíz del workspace, gitignored) y valida el token al
    // arrancar — si falta, panic claro antes de que cualquier IPC corra.
    let _ = dotenvy::dotenv();
    let _ = common::auth_token();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![fetch_events, clear_events])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
