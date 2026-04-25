mod browser_client;

use browser_client::BrowserClient;
use common::{EventKind, PageState};
use tauri::{Manager, State};

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
            browser_navigate,
            browser_event,
            browser_get_content
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
