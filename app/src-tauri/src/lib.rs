use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use classifier_core::{apply_filter_batch, Classifier};


/// Wraps an optional Classifier so we can boot the app even when the model
/// hasn't been exported yet (Phase 0). Filtering becomes passthrough in that
/// case; logs explain how to enable.
struct ClassifierHolder(Option<Arc<Classifier>>);

/// Locates the ONNX model + runtime config. Tries the bundled resource dir
/// first (production builds), then falls back to the dev path under
/// `classifier/onnx_model/`.
fn try_load_classifier(app: &tauri::App) -> anyhow::Result<Classifier> {
    let resource_dir = app.path().resource_dir().ok();

    let runtime_path = resource_dir
        .as_ref()
        .map(|r| r.join("runtime.json"))
        .filter(|p| p.exists())
        .or_else(|| {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime.json");
            p.exists().then_some(p)
        })
        .ok_or_else(|| anyhow::anyhow!("runtime.json no encontrado (corre scripts/gen_runtime.py)"))?;

    let onnx_dir = resource_dir
        .as_ref()
        .map(|r| r.join("onnx_model"))
        .filter(|p| p.exists())
        .or_else(|| {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()?
                .parent()?
                .join("classifier/onnx_model");
            p.exists().then_some(p)
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "onnx_model/ no encontrado. Corre: cd classifier && uv run --extra export python src/export.py",
            )
        })?;

    let model_path = onnx_dir.join("model.onnx");
    let tokenizer_path = onnx_dir.join("tokenizer.json");
    let meta_path = onnx_dir.join("meta.json");

    Classifier::new(&runtime_path, &model_path, &tokenizer_path, &meta_path)
}


#[cfg(desktop)]
use tauri::{Emitter, LogicalPosition, LogicalSize, WebviewUrl, WebviewWindowBuilder};

#[cfg(desktop)]
const FILTER_SCRIPT: &str = include_str!("filter.js");
#[cfg(desktop)]
const BROWSER_PANE_LABEL: &str = "browser_pane";
#[cfg(desktop)]
const BROWSER_PANE_UA: &str =
    "Mozilla/5.0 (Android 13; Mobile; rv:128.0) Gecko/128.0 Firefox/128.0";

#[cfg(desktop)]
fn navigation_allowed(url: &url::Url) -> bool {
    let s = url.as_str().to_ascii_lowercase();
    const BAD: &[&str] = &[
        "porn",
        "xxx",
        "xvideos",
        "pornhub",
        "redtube",
        "youporn",
        "xnxx",
        "onlyfans",
        "chaturbate",
    ];
    !BAD.iter().any(|p| s.contains(p))
}

#[cfg(desktop)]
fn screen_position(app: &AppHandle, rel_x: f64, rel_y: f64) -> Result<(f64, f64), String> {
    let main = app.get_webview_window("main").ok_or("no main window")?;
    let scale = main.scale_factor().map_err(|e| e.to_string())?;
    let inner = main.inner_position().map_err(|e| e.to_string())?;
    let inner_logical = inner.to_logical::<f64>(scale);
    Ok((inner_logical.x + rel_x, inner_logical.y + rel_y))
}

#[tauri::command]
async fn open_browser_view(
    app: AppHandle,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    #[cfg(desktop)]
    {
        let _ = url::Url::parse(&url).map_err(|e| format!("invalid URL: {e}"))?;

        if let Some(existing) = app.get_webview_window(BROWSER_PANE_LABEL) {
            let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
            existing.navigate(parsed).map_err(|e| e.to_string())?;
            return Ok(());
        }

        let main = app.get_webview_window("main").ok_or("no main window")?;
        let parsed = url::Url::parse(&url).map_err(|e| format!("invalid URL: {e}"))?;
        let (screen_x, screen_y) = screen_position(&app, x, y)?;
        eprintln!(
            "[browser_pane] open(desktop): rel=({x:.1},{y:.1}) screen=({screen_x:.1},{screen_y:.1}) size=({width:.1}x{height:.1})"
        );
        let app_handle = app.clone();
        let mut builder =
            WebviewWindowBuilder::new(&app, BROWSER_PANE_LABEL, WebviewUrl::External(parsed))
                .initialization_script(FILTER_SCRIPT)
                .user_agent(BROWSER_PANE_UA)
                .decorations(false)
                .resizable(false)
                .position(screen_x, screen_y)
                .inner_size(width, height)
                .on_navigation(move |u| {
                    let allowed = navigation_allowed(u);
                    if allowed {
                        let _ = app_handle.emit("browser-navigated", u.to_string());
                    } else {
                        let _ = app_handle.emit("browser-blocked", u.to_string());
                    }
                    allowed
                });
        builder = builder.parent(&main).map_err(|e| e.to_string())?;
        builder.build().map_err(|e| e.to_string())?;
    }

    #[cfg(mobile)]
    {
        tauri_plugin_native_browser_pane::run(
            &app,
            "open",
            serde_json::json!({
                "url": url,
                "x": x,
                "y": y,
                "width": width,
                "height": height,
            }),
        )?;
    }

    Ok(())
}

#[tauri::command]
async fn navigate_browser_view(app: AppHandle, url: String) -> Result<(), String> {
    #[cfg(desktop)]
    {
        let win = app
            .get_webview_window(BROWSER_PANE_LABEL)
            .ok_or("browser pane not open")?;
        let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
        win.navigate(parsed).map_err(|e| e.to_string())?;
    }

    #[cfg(mobile)]
    {
        tauri_plugin_native_browser_pane::run(&app, "navigate", serde_json::json!({ "url": url }))?;
    }

    Ok(())
}

#[tauri::command]
async fn set_browser_view_bounds(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    #[cfg(desktop)]
    {
        let win = app
            .get_webview_window(BROWSER_PANE_LABEL)
            .ok_or("browser pane not open")?;
        let (screen_x, screen_y) = screen_position(&app, x, y)?;
        win.set_position(LogicalPosition::new(screen_x, screen_y))
            .map_err(|e| e.to_string())?;
        win.set_size(LogicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
    }

    #[cfg(mobile)]
    {
        tauri_plugin_native_browser_pane::run(
            &app,
            "setBounds",
            serde_json::json!({
                "x": x,
                "y": y,
                "width": width,
                "height": height,
            }),
        )?;
    }

    Ok(())
}

/// Filtra una lista de textos en una sola IPC. Cada texto pasa por el clasificador
/// zero-shot multi-hipótesis: si la decisión es BLOQUEAR se reemplazan los
/// alfanuméricos por █, si es AVISAR se reemplazan las letras por '-', y si es
/// PERMITIR se devuelve el texto sin cambios. Si el modelo no está cargado
/// (Phase 0 no corrida), todos los textos pasan sin cambios.
#[tauri::command]
async fn filter_texts(
    state: tauri::State<'_, ClassifierHolder>,
    texts: Vec<String>,
) -> Result<Vec<String>, String> {
    let classifier = state.0.clone();

    let result = tokio::task::spawn_blocking(move || match classifier {
        Some(c) => apply_filter_batch(&c, &texts),
        None => texts,
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(result)
}

/// Aplica blur Gaussiano a los bytes de una imagen y devuelve los bytes JPEG
/// resultantes via `tauri::ipc::Response` (transporte binario, no JSON / base64).
/// El JS hace el `fetch(img.src)` localmente (cache hit del browser) y nos pasa
/// los bytes — eliminamos el segundo network fetch desde Rust.
#[tauri::command]
async fn filter_image_bytes(bytes: Vec<u8>) -> Result<tauri::ipc::Response, String> {
    let blurred = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        // Downscale agresivo: el CSS aplica un blur de 24px encima hasta que
        // estos bytes lleguen, así que solo necesitamos algo irreversiblemente
        // borroso. 128px + sigma 8 es ~25x más barato que 512px + sigma 15.
        let resized = img.resize(128, 128, image::imageops::FilterType::Triangle);
        // fast_blur es separable Gaussian — ~5x más rápido que blur full-2D.
        let blurred = image::imageops::fast_blur(&resized.to_rgba8(), 8.0);
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(blurred)
            .to_rgb8()
            .write_to(&mut out, image::ImageFormat::Jpeg)
            .map_err(|e| e.to_string())?;
        Ok(out.into_inner())
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(tauri::ipc::Response::new(blurred))
}

#[tauri::command]
async fn close_browser_view(app: AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    {
        if let Some(win) = app.get_webview_window(BROWSER_PANE_LABEL) {
            win.close().map_err(|e| e.to_string())?;
        }
    }

    #[cfg(mobile)]
    {
        tauri_plugin_native_browser_pane::run(&app, "close", serde_json::json!({}))?;
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init());

    #[cfg(mobile)]
    {
        builder = builder.plugin(tauri_plugin_native_browser_pane::init());
    }

    builder
        .setup(|app| {
            let holder = match try_load_classifier(app) {
                Ok(c) => {
                    eprintln!("[classifier] cargado");
                    let arc = Arc::new(c);
                    // Comparte la instancia con el plugin nativo (ios_filter.rs)
                    // vía OnceLock global en classifier-core, así no se duplica
                    // el modelo en memoria ni se reintenta resolver el bundle path.
                    let _ = classifier_core::SHARED_CLASSIFIER.set(arc.clone());
                    ClassifierHolder(Some(arc))
                }
                Err(e) => {
                    eprintln!("[classifier] desactivado: {e}");
                    ClassifierHolder(None)
                }
            };
            app.manage(holder);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_browser_view,
            navigate_browser_view,
            set_browser_view_bounds,
            close_browser_view,
            filter_texts,
            filter_image_bytes
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
