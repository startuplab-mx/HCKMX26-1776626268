use common::{EventKind, EventRequest, NavigateRequest, PageState};
use reqwest::header::AUTHORIZATION;

pub struct BrowserClient {
    http: reqwest::Client,
    base: String,
}

impl BrowserClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .build()
            .expect("reqwest client build");
        Self {
            http,
            base: common::service_url(),
        }
    }

    pub async fn navigate(&self, url: String) -> Result<PageState, String> {
        let body = NavigateRequest { url };
        let resp = self
            .http
            .post(format!("{}/navigate", self.base))
            .header(AUTHORIZATION, common::bearer_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        decode(resp).await
    }

    pub async fn event(
        &self,
        kind: EventKind,
        selector: String,
        value: Option<String>,
    ) -> Result<PageState, String> {
        let body = EventRequest {
            kind,
            selector,
            value,
        };
        let resp = self
            .http
            .post(format!("{}/event", self.base))
            .header(AUTHORIZATION, common::bearer_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        decode(resp).await
    }

    pub async fn content(&self) -> Result<PageState, String> {
        let resp = self
            .http
            .get(format!("{}/content", self.base))
            .header(AUTHORIZATION, common::bearer_header())
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        decode(resp).await
    }
}

async fn decode(resp: reqwest::Response) -> Result<PageState, String> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("browser returned {status}: {body}"));
    }
    resp.json::<PageState>()
        .await
        .map_err(|e| format!("decoding response failed: {e}"))
}
