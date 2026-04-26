//! Server Actix que recibe `FilterEvent`s de la app (POST /events) y los
//! sirve al dashboard (GET /events). Auth por bearer token compartido
//! (token compartido vía `common::auth_token()`).

mod middleware;

use std::collections::VecDeque;
use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use parking_lot::RwLock;

use common::FilterEvent;

use crate::middleware::TokenAuth;

const MAX_EVENTS: usize = 1000;

#[derive(Clone, Default)]
struct AppState {
    events: Arc<RwLock<VecDeque<FilterEvent>>>,
}

async fn ingest(
    state: web::Data<AppState>,
    body: web::Json<FilterEvent>,
) -> impl Responder {
    let event = body.into_inner();
    let mut events = state.events.write();
    if events.len() >= MAX_EVENTS {
        events.pop_front();
    }
    events.push_back(event);
    HttpResponse::Accepted().json(serde_json::json!({ "ok": true }))
}

async fn list(
    state: web::Data<AppState>,
    q: web::Query<ListQuery>,
) -> impl Responder {
    let events = state.events.read();
    // Sin `since` devolvemos todo; con `since` sólo eventos después de ese
    // timestamp_ms (para polling incremental desde el dashboard).
    let since = q.since.unwrap_or(i64::MIN);
    let out: Vec<FilterEvent> = events
        .iter()
        .filter(|e| e.timestamp_ms > since)
        .cloned()
        .collect();
    HttpResponse::Ok().json(out)
}

async fn clear(state: web::Data<AppState>) -> impl Responder {
    state.events.write().clear();
    HttpResponse::NoContent().finish()
}

async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

#[derive(serde::Deserialize)]
struct ListQuery {
    since: Option<i64>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Carga `.env` (gitignored) — incluye `SHIELD_AUTH_TOKEN`. Si no
    // existe, las variables ya exportadas en el shell siguen funcionando.
    let _ = dotenvy::dotenv();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Forza el read del token al arrancar — si falta, panic ruidoso aquí
    // antes de bindear el socket en lugar de en el primer request.
    let _ = common::auth_token();

    let state = AppState::default();
    let bind = std::env::var("SERVER_BIND").unwrap_or_else(|_| "127.0.0.1:7878".to_string());

    log::info!("server escuchando en http://{bind}");

    HttpServer::new(move || {
        // CORS abierto para desarrollo — la app y el dashboard corren en
        // origens distintos (Tauri webview = http://tauri.localhost o file://).
        let cors = Cors::permissive();

        App::new()
            .app_data(web::Data::new(state.clone()))
            .app_data(web::JsonConfig::default().limit(1 * 1024 * 1024))
            .wrap(cors)
            // Health no requiere token — útil para readiness checks.
            .route("/health", web::get().to(health))
            .service(
                web::scope("")
                    .wrap(TokenAuth)
                    .route("/events", web::post().to(ingest))
                    .route("/events", web::get().to(list))
                    .route("/events", web::delete().to(clear)),
            )
    })
    .bind(bind)?
    .run()
    .await
}
