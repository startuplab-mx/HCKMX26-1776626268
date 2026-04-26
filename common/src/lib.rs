//! Tipos y constantes compartidos entre `app`, `server` y `dashboard`.
//!
//! **Política de secretos:** el bearer token NUNCA vive como literal en
//! el repo. Se lee por entorno (`SHIELD_AUTH_TOKEN`); cada binario
//! llama a [`auth_token()`] al startup para forzar un fallo temprano
//! si el `.env` no fue configurado. Ver `.env.example` en la raíz.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Header HTTP que transporta el token.
pub const AUTH_HEADER: &str = "Authorization";

/// Endpoint por defecto del server (sólo usado como fallback en dev).
pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:7878";

/// Variables de entorno que comparten los tres componentes.
pub const ENV_AUTH_TOKEN: &str = "SHIELD_AUTH_TOKEN";
pub const ENV_SERVER_URL: &str = "SHIELD_SERVER_URL";

/// Lee el token bearer. Precedencia:
///
/// 1. `std::env::var(SHIELD_AUTH_TOKEN)` — runtime, útil para CI/prod o
///    para overridear sin recompilar (incluyendo `dotenvy::dotenv()`
///    cargado al startup por el binario huésped).
/// 2. `SHIELD_AUTH_TOKEN_BUILD` — horneado por `common/build.rs` desde
///    el `.env` de la raíz del workspace al compilar. Necesario para
///    bundles (iOS/macOS app) donde el cwd no es el workspace.
///
/// Cachea el resultado en un `OnceLock`. Si ambas fuentes están vacías,
/// `panic!` ruidoso al primer uso para no arrancar en estado inseguro.
pub fn auth_token() -> &'static str {
    static CELL: OnceLock<String> = OnceLock::new();
    CELL.get_or_init(|| {
        if let Ok(t) = std::env::var(ENV_AUTH_TOKEN) {
            if !t.is_empty() {
                return t;
            }
        }
        let baked = env!("SHIELD_AUTH_TOKEN_BUILD");
        if baked.is_empty() {
            panic!(
                "falta {ENV_AUTH_TOKEN}. Defínelo en `.env` (raíz del repo, ver `.env.example`) \
                 y recompila, o exporta la variable antes de arrancar el proceso."
            );
        }
        baked.to_string()
    })
}

/// URL del server Actix: lee `SHIELD_SERVER_URL` del entorno con
/// fallback a [`DEFAULT_SERVER_URL`] para desarrollo local.
pub fn server_url() -> String {
    std::env::var(ENV_SERVER_URL).unwrap_or_else(|_| DEFAULT_SERVER_URL.to_string())
}

/// Coordenadas del elemento filtrado dentro de la WebView, en CSS pixels
/// relativos al viewport. Permiten al dashboard pintar el evento en su
/// posición real (heatmap, overlay, lista ordenable por zona).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coords {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Tipo de elemento filtrado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterKind {
    Text,
    Image,
}

/// Acción que tomó el clasificador sobre el contenido.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterAction {
    Allow,
    Warn,
    Block,
}

/// Evento que la app emite cuando el clasificador actúa sobre un elemento.
/// El server lo guarda en su buffer y el dashboard lo consume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterEvent {
    pub id: String,
    /// Texto/imagen kind.
    pub kind: FilterKind,
    /// Decisión del clasificador.
    pub action: FilterAction,
    /// Texto original (truncado para imágenes a algo tipo "img:<host>/<path>").
    pub original: String,
    /// Texto resultante tras aplicar el filtro (vacío si action=allow).
    pub filtered: String,
    /// Categorías que dispararon la decisión, si las hay.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Posición del elemento en la página.
    pub coords: Coords,
    /// URL del documento donde ocurrió el filtrado.
    pub url: String,
    /// Unix millis cuando se generó el evento.
    pub timestamp_ms: i64,
}
