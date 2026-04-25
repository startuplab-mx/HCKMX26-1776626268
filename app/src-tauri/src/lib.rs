mod browser_client;

use browser_client::BrowserClient;
use common::{EventKind, PageState};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, WebviewUrl,
    WebviewWindowBuilder,
};

const FILTER_SCRIPT: &str = include_str!("filter.js");
const BROWSER_PANE_LABEL: &str = "browser_pane";
const BROWSER_PANE_UA: &str =
    "Mozilla/5.0 (Android 13; Mobile; rv:128.0) Gecko/128.0 Firefox/128.0";

fn navigation_allowed(url: &url::Url) -> bool {
    let s = url.as_str().to_ascii_lowercase();
    const BAD: &[&str] = &[
        "porn", "xxx", "xvideos", "pornhub", "redtube", "youporn", "xnxx",
        "onlyfans", "chaturbate",
    ];
    !BAD.iter().any(|p| s.contains(p))
}

#[cfg(desktop)]
fn screen_position(
    app: &AppHandle,
    rel_x: f64,
    rel_y: f64,
) -> Result<(f64, f64), String> {
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
    #[allow(unused_variables)] x: f64,
    #[allow(unused_variables)] y: f64,
    #[allow(unused_variables)] width: f64,
    #[allow(unused_variables)] height: f64,
) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|e| format!("invalid URL: {e}"))?;

    // Si ya existe, simplemente navegamos. Esto es importante en iOS donde no
    // hay API pública para cerrar/destruir un WebviewWindow.
    if let Some(existing) = app.get_webview_window(BROWSER_PANE_LABEL) {
        existing.navigate(parsed).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let app_handle = app.clone();

    let mut builder = WebviewWindowBuilder::new(&app, BROWSER_PANE_LABEL, WebviewUrl::External(parsed))
        .initialization_script(FILTER_SCRIPT)
        .user_agent(BROWSER_PANE_UA)
        .on_navigation(move |u| {
            let allowed = navigation_allowed(u);
            if allowed {
                let _ = app_handle.emit("browser-navigated", u.to_string());
            } else {
                let _ = app_handle.emit("browser-blocked", u.to_string());
            }
            allowed
        });

    #[cfg(desktop)]
    {
        let main = app
            .get_webview_window("main")
            .ok_or("no main window")?;
        let (screen_x, screen_y) = screen_position(&app, x, y)?;
        eprintln!(
            "[browser_pane] open(desktop): rel=({x:.1},{y:.1}) screen=({screen_x:.1},{screen_y:.1}) size=({width:.1}x{height:.1})"
        );
        builder = builder
            .decorations(false)
            .resizable(false)
            .position(screen_x, screen_y)
            .inner_size(width, height)
            .parent(&main)
            .map_err(|e| e.to_string())?;
    }

    builder.build().map_err(|e| {
        eprintln!("[browser_pane] BUILD FAILED: {e}");
        e.to_string()
    })?;

    Ok(())
}

#[tauri::command]
async fn navigate_browser_view(app: AppHandle, url: String) -> Result<(), String> {
    let win = app
        .get_webview_window(BROWSER_PANE_LABEL)
        .ok_or("browser pane not open")?;
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    win.navigate(parsed).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn set_browser_view_bounds(
    #[allow(unused_variables)] app: AppHandle,
    #[allow(unused_variables)] x: f64,
    #[allow(unused_variables)] y: f64,
    #[allow(unused_variables)] width: f64,
    #[allow(unused_variables)] height: f64,
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
    Ok(())
}

#[tauri::command]
async fn close_browser_view(#[allow(unused_variables)] app: AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    if let Some(win) = app.get_webview_window(BROWSER_PANE_LABEL) {
        win.close().map_err(|e| e.to_string())?;
    }
    // En iOS/Android no hay API pública para cerrar; el WebviewWindow se
    // reutiliza navegándolo a otra URL o queda hasta que la app se cierre.
    Ok(())
}

// --- Comandos legacy del proxy/iframe (se mantienen por compatibilidad). ---

#[tauri::command]
async fn browser_navigate(
    state: State<'_, BrowserClient>,
    url: String,
) -> Result<PageState, String> {
    state.navigate(url).await
}

#[tauri::command]
async fn browser_event(
    state: State<'_, BrowserClient>,
    kind: String,
    selector: String,
    value: Option<String>,
) -> Result<PageState, String> {
    let kind = parse_kind(&kind)?;
    state.event(kind, selector, value).await
}

#[tauri::command]
async fn browser_get_content(state: State<'_, BrowserClient>) -> Result<PageState, String> {
    state.content().await
}

fn parse_kind(s: &str) -> Result<EventKind, String> {
    match s {
        "click" => Ok(EventKind::Click),
        "input" => Ok(EventKind::Input),
        "change" => Ok(EventKind::Change),
        "submit" => Ok(EventKind::Submit),
        "key" => Ok(EventKind::Key),
        other => Err(format!("unknown event kind: {other}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(BrowserClient::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_browser_view,
            navigate_browser_view,
            set_browser_view_bounds,
            close_browser_view,
            browser_navigate,
            browser_event,
            browser_get_content
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
