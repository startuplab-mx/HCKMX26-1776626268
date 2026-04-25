use std::sync::Arc;
use std::time::Duration;

use actix_web::http::header;
use actix_web::middleware::from_fn;
use actix_web::{HttpResponse, Responder, error, web};
use bytes::Bytes;
use common::{EventKind, EventRequest, NavigateRequest, PageState};
use playwright_rs::{APIRequestContext, Page, WaitUntil};
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::auth::require_token;
use crate::cache::{AssetCache, CachedAsset};
use crate::rewrite::{rewrite_css, rewrite_html};

pub struct AppState {
    pub page: Mutex<Page>,
    pub cache: Arc<AssetCache>,
    pub api_request: Option<Arc<APIRequestContext>>,
}

#[derive(Deserialize)]
pub struct AssetQuery {
    pub u: String,
    pub t: String,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(health));
    cfg.route("/asset", web::get().to(asset));
    cfg.service(
        web::scope("")
            .wrap(from_fn(require_token))
            .route("/navigate", web::post().to(navigate))
            .route("/event", web::post().to(event))
            .route("/content", web::get().to(content)),
    );
}

async fn health() -> impl Responder {
    HttpResponse::Ok().body("ok")
}

async fn asset(
    state: web::Data<AppState>,
    query: web::Query<AssetQuery>,
) -> actix_web::Result<HttpResponse> {
    if query.t != common::APP_TOKEN {
        return Err(error::ErrorUnauthorized("invalid token"));
    }
    let url = query.u.clone();

    if is_tracker(&url) {
        return Ok(HttpResponse::NoContent().finish());
    }

    if let Some(asset) = state.cache.get(&url) {
        return Ok(serve_asset(asset));
    }

    let api = state.api_request.clone().ok_or_else(|| {
        error::ErrorBadGateway("asset not in cache and no live fallback available")
    })?;

    let resp = api
        .fetch(&url, None)
        .await
        .map_err(|e| error::ErrorBadGateway(format!("live fetch failed: {e}")))?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let body = resp
        .body()
        .await
        .map_err(|e| error::ErrorBadGateway(format!("live body failed: {e}")))?;
    let body = if content_type.to_ascii_lowercase().contains("text/css") {
        match std::str::from_utf8(&body) {
            Ok(s) => rewrite_css(s, &url).into_bytes(),
            Err(_) => body,
        }
    } else {
        body
    };
    let asset = CachedAsset {
        status,
        content_type,
        body: Bytes::from(body),
    };
    state.cache.insert(url, asset.clone());
    Ok(serve_asset(asset))
}

fn is_tracker(url: &str) -> bool {
    const TRACKERS: &[&str] = &[
        // Google
        "play.google.com/log",
        "ogads-pa.clients6.google.com",
        "clients6.google.com/$rpc",
        "google-analytics.com",
        "googletagmanager.com",
        "googleadservices.com",
        "doubleclick.net",
        "google.com/gen_204",
        "youtube.com/api/stats",
        // Facebook
        "facebook.com/ajax/bz",
        "facebook.com/ajax/log",
        "facebook.com/ai.php",
        "facebook.com/security/hsts_pixel",
        "connect.facebook.net",
        "/banzai/log",
        "/qpl_log",
        // Genéricos
        "/log?",
        "/jserror",
        "/csi?",
    ];
    let lower = url.to_ascii_lowercase();
    TRACKERS.iter().any(|p| lower.contains(p))
}

fn serve_asset(asset: CachedAsset) -> HttpResponse {
    let mut builder = HttpResponse::build(
        actix_web::http::StatusCode::from_u16(asset.status).unwrap_or(actix_web::http::StatusCode::OK),
    );
    if !asset.content_type.is_empty() {
        builder.insert_header((header::CONTENT_TYPE, asset.content_type.clone()));
    }
    builder.insert_header((header::CACHE_CONTROL, "private, max-age=300"));
    builder.body(asset.body)
}

async fn navigate(
    state: web::Data<AppState>,
    body: web::Json<NavigateRequest>,
) -> actix_web::Result<web::Json<PageState>> {
    let page = state.page.lock().await;
    info!(url = %body.url, "navigate");
    page.goto(&body.url, None)
        .await
        .map_err(|e| error::ErrorBadGateway(format!("goto failed: {e}")))?;
    // Espera adaptativa: NetworkIdle con cap de 3s. En páginas livianas regresa
    // casi inmediato; en SPAs pesados (Bloks, React, etc.) deja hasta 3s para
    // que el JS termine de renderizar el DOM antes de snapshotear. Si nunca
    // llega a idle (long-poll, websockets), tokio::timeout corta sin error.
    let _ = tokio::time::timeout(
        Duration::from_millis(3000),
        page.wait_for_load_state(Some(WaitUntil::NetworkIdle)),
    )
    .await;
    // Buffer de paint: una vez idle puede haber un último tick de hydration.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let snapshot = snapshot(&page).await?;
    Ok(web::Json(snapshot))
}

async fn event(
    state: web::Data<AppState>,
    body: web::Json<EventRequest>,
) -> actix_web::Result<web::Json<PageState>> {
    let page = state.page.lock().await;
    let EventRequest {
        kind,
        selector,
        value,
    } = body.into_inner();
    info!(?kind, %selector, "event");

    let locator = page.locator(&selector).await;
    match kind {
        EventKind::Click => {
            locator
                .click(None)
                .await
                .map_err(|e| error::ErrorBadGateway(format!("click failed: {e}")))?;
        }
        EventKind::Input | EventKind::Change => {
            let text = value.unwrap_or_default();
            locator
                .fill(&text, None)
                .await
                .map_err(|e| error::ErrorBadGateway(format!("fill failed: {e}")))?;
        }
        EventKind::Submit => {
            locator
                .evaluate::<serde_json::Value, ()>(
                    "form => { if (form && typeof form.requestSubmit === 'function') { form.requestSubmit(); } else if (form) { form.submit(); } }",
                    None,
                )
                .await
                .map_err(|e| error::ErrorBadGateway(format!("submit failed: {e}")))?;
        }
        EventKind::Key => {
            let key = value.unwrap_or_else(|| "Enter".into());
            locator
                .press(&key, None)
                .await
                .map_err(|e| error::ErrorBadGateway(format!("press failed: {e}")))?;
        }
    }

    let snapshot = snapshot(&page).await?;
    Ok(web::Json(snapshot))
}

async fn content(state: web::Data<AppState>) -> actix_web::Result<web::Json<PageState>> {
    let page = state.page.lock().await;
    let snapshot = snapshot(&page).await?;
    Ok(web::Json(snapshot))
}

async fn snapshot(page: &Page) -> actix_web::Result<PageState> {
    let html = page
        .content()
        .await
        .map_err(|e| error::ErrorBadGateway(format!("content failed: {e}")))?;
    let title = page.title().await.unwrap_or_else(|err| {
        warn!(error = %err, "title failed; using empty string");
        String::new()
    });
    let url = page.url();
    let html = rewrite_html(&html, &url)
        .map_err(|e| error::ErrorBadGateway(format!("rewrite_html failed: {e}")))?;
    Ok(PageState { url, title, html })
}
