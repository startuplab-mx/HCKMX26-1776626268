use tauri::{AppHandle, Manager};

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
        .invoke_handler(tauri::generate_handler![
            open_browser_view,
            navigate_browser_view,
            set_browser_view_bounds,
            close_browser_view
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
