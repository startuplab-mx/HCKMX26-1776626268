mod auth;
mod cache;
mod camoufox;
mod rewrite;
mod server;

use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{App, HttpServer, web};
use anyhow::Context;
use bytes::Bytes;
use playwright_rs::{BrowserContextOptions, LaunchOptions, Playwright, Viewport};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

use crate::cache::{AssetCache, CachedAsset};
use crate::server::AppState;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let camoufox_path = camoufox::ensure_camoufox().await?;
    let executable_path = camoufox_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("camoufox path is not valid UTF-8"))?
        .to_owned();

    info!("launching playwright + camoufox");
    let pw = Playwright::launch().await.context("playwright launch")?;
    let browser = pw
        .firefox()
        .launch_with_options(LaunchOptions {
            executable_path: Some(executable_path),
            ..Default::default()
        })
        .await
        .context("camoufox launch")?;
    // Contexto mobile: viewport y UA de Firefox Android. Sirve para que servidores
    // con detección de UA devuelvan HTML mobile (más ligero y mejor adaptado al
    // espacio del iframe), y para que las media queries client-side activen los
    // breakpoints mobile (≤ 768px).
    let context = browser
        .new_context_with_options(BrowserContextOptions {
            viewport: Some(Viewport {
                width: 412,
                height: 915,
            }),
            user_agent: Some(
                "Mozilla/5.0 (Android 13; Mobile; rv:128.0) Gecko/128.0 Firefox/128.0".into(),
            ),
            ..Default::default()
        })
        .await
        .context("new_context_with_options")?;
    let page = context.new_page().await.context("new_page")?;
    page.set_default_timeout(5000.0).await;
    page.set_default_navigation_timeout(20000.0).await;

    let cache = Arc::new(AssetCache::new());
    {
        let cache_handler = cache.clone();
        page.on_response(move |response| {
            let cache = cache_handler.clone();
            async move {
                let url = response.url().to_string();
                if url.is_empty() {
                    return Ok(());
                }
                let status = response.status();
                let content_type = match response.raw_headers().await {
                    Ok(headers) => headers
                        .iter()
                        .find(|h| h.name.eq_ignore_ascii_case("content-type"))
                        .map(|h| h.value.clone())
                        .unwrap_or_default(),
                    Err(_) => String::new(),
                };
                let body = match response.body().await {
                    Ok(b) => b,
                    Err(err) => {
                        debug!(%url, %err, "skipping body capture");
                        return Ok(());
                    }
                };
                let body = if content_type.to_ascii_lowercase().contains("text/css") {
                    let text = match std::str::from_utf8(&body) {
                        Ok(s) => s.to_string(),
                        Err(_) => return Ok(()),
                    };
                    rewrite::rewrite_css(&text, &url).into_bytes()
                } else {
                    body
                };
                cache.insert(
                    url,
                    CachedAsset {
                        status,
                        content_type,
                        body: Bytes::from(body),
                    },
                );
                Ok(())
            }
        })
        .await
        .context("registering on_response")?;
    }

    let api_request = match page.context() {
        Ok(ctx) => match ctx.request().await {
            Ok(req) => Some(req),
            Err(e) => {
                warn!(error = %e, "could not obtain APIRequestContext; live fallback disabled");
                None
            }
        },
        Err(e) => {
            warn!(error = %e, "could not obtain BrowserContext; live fallback disabled");
            None
        }
    };

    let state = web::Data::new(AppState {
        page: Mutex::new(page),
        cache,
        api_request: api_request.map(Arc::new),
    });

    info!(
        "sandbox-browser listening on http://{}:{}",
        common::SERVICE_HOST,
        common::SERVICE_PORT
    );
    HttpServer::new(move || {
        App::new()
            .wrap(
                Cors::default()
                    .allowed_origin_fn(|_origin, _req_head| true)
                    .allow_any_method()
                    .allow_any_header()
                    .supports_credentials()
                    .max_age(3600),
            )
            .app_data(web::JsonConfig::default().limit(8 * 1024 * 1024))
            .app_data(state.clone())
            .configure(server::configure)
    })
    .workers(1)
    .bind((common::SERVICE_HOST, common::SERVICE_PORT))?
    .run()
    .await?;

    Ok(())
}
