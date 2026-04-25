//! Tauri plugin para añadir un WKWebView (iOS) como subview del UIViewController
//! principal — un browser pane *embebido* y posicionable, para Tauri 2 mobile.
//!
//! En iOS, registra `NativeBrowserPanePlugin.swift`. En desktop es no-op (el
//! wiring desktop usa `WebviewWindow` directamente desde el código de la app).

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_native_browser_pane);

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.hackathon404.nativebrowserpane";

#[cfg(mobile)]
pub struct BrowserPane<R: Runtime>(pub tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("native-browser-pane")
        .setup(|_app, _api| {
            #[cfg(target_os = "ios")]
            {
                let handle = _api.register_ios_plugin(init_plugin_native_browser_pane)?;
                _app.manage(BrowserPane(handle));
            }
            #[cfg(target_os = "android")]
            {
                let handle = _api
                    .register_android_plugin(PLUGIN_IDENTIFIER, "NativeBrowserPanePlugin")?;
                _app.manage(BrowserPane(handle));
            }
            Ok(())
        })
        .build()
}

/// Llama a un método del plugin nativo iOS. En desktop retorna un error: la
/// app debe usar `WebviewWindow` directamente.
#[cfg(mobile)]
pub fn run<R: Runtime, M: Manager<R>>(
    manager: &M,
    method: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let state = manager
        .try_state::<BrowserPane<R>>()
        .ok_or("native-browser-pane plugin not initialized")?;
    state
        .0
        .run_mobile_plugin::<serde_json::Value>(method, payload)
        .map_err(|e| e.to_string())
}

#[cfg(not(mobile))]
pub fn run<R: Runtime, M: Manager<R>>(
    _manager: &M,
    _method: &str,
    _payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    Err("native-browser-pane is mobile-only; use WebviewWindow on desktop".into())
}
