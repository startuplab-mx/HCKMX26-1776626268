use serde::{Deserialize, Serialize};

pub const APP_TOKEN: &str =
    "f4c1d8e2-7a5b-4cf3-9d61-0b8e6a3c7f2d-sandbox-browser-dev-token";

pub const SERVICE_HOST: &str = "127.0.0.1";
pub const SERVICE_PORT: u16 = 8765;

pub fn service_url() -> String {
    format!("http://{SERVICE_HOST}:{SERVICE_PORT}")
}

pub fn bearer_header() -> String {
    format!("Bearer {APP_TOKEN}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigateRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageState {
    pub url: String,
    pub title: String,
    pub html: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Click,
    Input,
    Change,
    Submit,
    Key,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRequest {
    pub kind: EventKind,
    pub selector: String,
    #[serde(default)]
    pub value: Option<String>,
}
