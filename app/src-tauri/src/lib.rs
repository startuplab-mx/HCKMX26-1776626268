use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use classifier::{
    obscure_dashes, obscure_full, Action, Classifier, ImageClassifier, ImageDecision,
    MIN_TEXT_LEN_FOR_CLASSIFY,
};
use common::{auth_token, server_url, Coords, FilterAction, FilterEvent, FilterKind};


/// Wraps an optional Classifier so we can boot the app even when the model
/// hasn't been exported yet (Phase 0). Filtering becomes passthrough en ese
/// caso; logs explican cómo habilitar.
struct ClassifierHolder(Option<Arc<Classifier>>);

/// Análogo para el clasificador zero-shot de imágenes (MobileCLIP S1).
/// Si los archivos no están bundled, dejamos `None` y caemos al modo
/// "blur all" — el comportamiento previo a la integración del modelo.
struct ImageClassifierHolder(Option<Arc<ImageClassifier>>);

/// Cliente HTTP que reporta cada `FilterEvent` (no permitido) al server
/// Actix. Spawns un task tokio para no bloquear el hot path del filtrado.
/// Si la URL del server no está configurada o el server está abajo, los
/// errores se loggean y se ignoran — la app no debe romper porque el
/// dashboard no esté corriendo.
#[derive(Clone)]
struct EventEmitter {
    client: reqwest::Client,
    server_url: String,
}

impl EventEmitter {
    fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            server_url: server_url(),
        }
    }

    fn emit(&self, event: FilterEvent) {
        let client = self.client.clone();
        let url = format!("{}/events", self.server_url);
        tokio::spawn(async move {
            let resp = client
                .post(&url)
                .bearer_auth(auth_token())
                .json(&event)
                .send()
                .await;
            if let Err(e) = resp {
                eprintln!("[event-emitter] post falló: {e}");
            }
        });
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn new_event_id() -> String {
    use rand::Rng;
    let r: u64 = rand::thread_rng().gen();
    format!("{:016x}", r)
}

fn action_to_filter_action(a: Action) -> FilterAction {
    match a {
        Action::Permitir => FilterAction::Allow,
        Action::Avisar => FilterAction::Warn,
        Action::Bloquear => FilterAction::Block,
    }
}

fn obscure_for(action: Action, text: &str) -> String {
    match action {
        Action::Bloquear => obscure_full(text),
        Action::Avisar => obscure_dashes(text),
        Action::Permitir => text.to_string(),
    }
}

/// Locates the ONNX model + runtime config. Tries the bundled resource dir
/// first (production builds), then falls back to the dev path under
/// `classifier-py/onnx_model/`.
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
                .join("classifier-py/onnx_model");
            p.exists().then_some(p)
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "onnx_model/ no encontrado. Corre: cd classifier-py && uv run --extra export python src/export.py",
            )
        })?;

    let model_path = onnx_dir.join("model.onnx");
    let tokenizer_path = onnx_dir.join("tokenizer.json");
    let meta_path = onnx_dir.join("meta.json");

    Classifier::new(&runtime_path, &model_path, &tokenizer_path, &meta_path)
}

/// Localiza los archivos del clasificador de imágenes MobileCLIP. Igual que
/// `try_load_classifier`: bundled resource dir primero, dev path como fallback.
fn try_load_image_classifier(app: &tauri::App) -> anyhow::Result<ImageClassifier> {
    let resource_dir = app.path().resource_dir().ok();

    let dir = resource_dir
        .as_ref()
        .map(|r| r.join("mobileclip"))
        .filter(|p| p.exists())
        .or_else(|| {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/mobileclip");
            p.exists().then_some(p)
        })
        .ok_or_else(|| anyhow::anyhow!("resources/mobileclip/ no encontrado"))?;

    let model_path = dir.join("mobileclip_image.onnx");
    let anchors_path = dir.join("text_features_anchors.npy");
    if !model_path.exists() {
        return Err(anyhow::anyhow!(
            "falta {} (mueve los archivos del compañero ahí)",
            model_path.display()
        ));
    }
    if !anchors_path.exists() {
        return Err(anyhow::anyhow!(
            "falta {} (mueve los archivos del compañero ahí)",
            anchors_path.display()
        ));
    }
    ImageClassifier::new(&model_path, &anchors_path)
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

/// Item con texto + coordenadas del elemento (CSS px relativos al viewport
/// de la WebView). Las coordenadas viajan al dashboard para ubicar el evento
/// en su posición real (heatmap / overlay).
#[derive(Deserialize)]
struct TextItem {
    text: String,
    coords: Coords,
}

/// Filtra una lista de textos en una sola IPC. Cada texto pasa por el clasificador
/// zero-shot multi-hipótesis: si la decisión es BLOQUEAR se reemplazan los
/// alfanuméricos por █, si es AVISAR se reemplazan las letras por '-', y si es
/// PERMITIR se devuelve el texto sin cambios. Si el modelo no está cargado
/// (Phase 0 no corrida), todos los textos pasan sin cambios.
///
/// Por cada decisión != Permitir emite un `FilterEvent` al server con las
/// coordenadas del elemento para que el dashboard lo muestre.
#[tauri::command]
async fn filter_texts(
    state: tauri::State<'_, ClassifierHolder>,
    emitter: tauri::State<'_, EventEmitter>,
    items: Vec<TextItem>,
    page_url: String,
) -> Result<Vec<String>, String> {
    let classifier = state.0.clone();
    let emitter = emitter.inner().clone();

    let result = tokio::task::spawn_blocking(move || -> Vec<String> {
        // Sin modelo: passthrough de todos los textos (sin eventos).
        let Some(classifier) = classifier else {
            return items.into_iter().map(|i| i.text).collect();
        };

        let mut results: Vec<String> = vec![String::new(); items.len()];
        let mut to_classify: Vec<String> = Vec::with_capacity(items.len());
        let mut idx_map: Vec<usize> = Vec::with_capacity(items.len());

        for (i, item) in items.iter().enumerate() {
            if item.text.trim().chars().count() < MIN_TEXT_LEN_FOR_CLASSIFY {
                results[i] = item.text.clone();
            } else {
                idx_map.push(i);
                to_classify.push(item.text.clone());
            }
        }

        if to_classify.is_empty() {
            return results;
        }

        match classifier.classify_many(&to_classify, &[]) {
            Ok(decisions) => {
                for (k, decision) in decisions.into_iter().enumerate() {
                    let i = idx_map[k];
                    let original = &to_classify[k];
                    let filtered = obscure_for(decision.action.clone(), original);
                    results[i] = filtered.clone();

                    // Emite evento sólo para decisiones disruptivas. Permitir
                    // se ignora — saturaría al server sin valor para el dashboard.
                    if !matches!(decision.action, Action::Permitir) {
                        emitter.emit(FilterEvent {
                            id: new_event_id(),
                            kind: FilterKind::Text,
                            action: action_to_filter_action(decision.action),
                            original: original.clone(),
                            filtered,
                            categories: decision.categories,
                            coords: items[i].coords.clone(),
                            url: page_url.clone(),
                            timestamp_ms: now_ms(),
                        });
                    }
                }
            }
            Err(e) => {
                eprintln!("[classifier] error (batch): {e}");
                for (k, text) in to_classify.into_iter().enumerate() {
                    results[idx_map[k]] = text;
                }
            }
        }

        results
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(result)
}

/// Decide si una imagen es benigna o de riesgo usando MobileCLIP. Si es
/// benigna devuelve los bytes originales tal cual; si es de riesgo (o si el
/// clasificador no está cargado / falla) aplica blur agresivo y devuelve los
/// bytes JPEG resultantes. La respuesta viaja via `tauri::ipc::Response`
/// (binario, sin base64) y el JS reemplaza el `src` del `<img>` con el blob.
///
/// Política fail-closed: si decode/inferencia revientan, devolvemos imagen
/// borrosa por seguridad — preferimos un falso positivo a exponer contenido
/// no clasificado.
#[tauri::command]
async fn filter_image_bytes(
    state: tauri::State<'_, ImageClassifierHolder>,
    emitter: tauri::State<'_, EventEmitter>,
    bytes: Vec<u8>,
    coords: Coords,
    page_url: String,
    image_url: String,
) -> Result<tauri::ipc::Response, String> {
    let classifier = state.0.clone();
    let emitter = emitter.inner().clone();

    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        // 1. Clasificar. Sin clasificador cargado caemos al comportamiento
        //    legacy "blur todo" — más conservador que dejar pasar imágenes
        //    sin filtrar.
        let decision = match &classifier {
            Some(c) => c.classify(&bytes).unwrap_or_else(|e| {
                eprintln!("[image_classifier] error: {e} → bloqueando por seguridad");
                ImageDecision::Block
            }),
            None => ImageDecision::Block,
        };

        if matches!(decision, ImageDecision::Allow) {
            // Imagen benigna: pasar los bytes tal cual. El JS reemplaza el
            // src con el blob → el `__sb_done` queda aplicado y el CSS pre-hide
            // se levanta. Cero re-encoding, cero pérdida de calidad.
            return Ok(bytes);
        }

        // Emite evento al server con la URL de la imagen y la posición.
        emitter.emit(FilterEvent {
            id: new_event_id(),
            kind: FilterKind::Image,
            action: FilterAction::Block,
            original: image_url,
            filtered: String::from("[blurred]"),
            categories: Vec::new(),
            coords,
            url: page_url,
            timestamp_ms: now_ms(),
        });

        // 2. Imagen flaggeada: blur agresivo. 128px + sigma 8 es
        //    ~25x más barato que 512px + sigma 15 y queda irreversiblemente
        //    borroso a la vista del usuario.
        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        let resized = img.resize(128, 128, image::imageops::FilterType::Triangle);
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

    Ok(tauri::ipc::Response::new(result))
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
    // Carga `.env` (raíz del workspace, gitignored) — SHIELD_AUTH_TOKEN
    // y opcionalmente SHIELD_SERVER_URL. En mobile no hay filesystem
    // estándar / cwd estable, así que el `Err` se ignora — allá las vars
    // viajan vía build script o launch args.
    let _ = dotenvy::dotenv();
    // Forza la lectura del token aquí: si falta, queremos crashear al
    // iniciar con un mensaje claro, no en el primer evento de filtrado.
    let _ = common::auth_token();

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
            // En iOS enlazamos ort estáticamente con la feature
            // `alternative-backend`, que requiere inicializar la `OrtApi` global
            // explícitamente antes de cualquier uso de `ort`.
            #[cfg(target_os = "ios")]
            classifier::init_ort_api();

            let holder = match try_load_classifier(app) {
                Ok(c) => {
                    eprintln!("[classifier] cargado");
                    let arc = Arc::new(c);
                    // Comparte la instancia con el plugin nativo (ios_filter.rs)
                    // vía OnceLock global en classifier, así no se duplica
                    // el modelo en memoria ni se reintenta resolver el bundle path.
                    let _ = classifier::SHARED_CLASSIFIER.set(arc.clone());
                    ClassifierHolder(Some(arc))
                }
                Err(e) => {
                    eprintln!("[classifier] desactivado: {e}");
                    ClassifierHolder(None)
                }
            };
            app.manage(holder);

            let img_holder = match try_load_image_classifier(app) {
                Ok(c) => {
                    eprintln!("[image_classifier] cargado");
                    let arc = Arc::new(c);
                    let _ = classifier::SHARED_IMAGE_CLASSIFIER.set(arc.clone());
                    ImageClassifierHolder(Some(arc))
                }
                Err(e) => {
                    eprintln!("[image_classifier] desactivado (fallback blur-all): {e}");
                    ImageClassifierHolder(None)
                }
            };
            app.manage(img_holder);

            // EventEmitter: cliente HTTP que reporta cada decisión != Permitir
            // al server (Actix). El dashboard hace polling de /events.
            let emitter = EventEmitter::new();
            eprintln!("[event-emitter] server: {}", emitter.server_url);
            app.manage(emitter);
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
