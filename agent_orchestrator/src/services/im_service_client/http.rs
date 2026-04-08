use std::time::Duration;

use once_cell::sync::Lazy;
use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;

use crate::config::Config;

use super::current_access_token;

static IM_SERVICE_HTTP: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

pub fn client() -> &'static reqwest::Client {
    &IM_SERVICE_HTTP
}

pub fn build_url(path: &str) -> String {
    format!(
        "{}{}",
        Config::get().im_service_base_url.trim_end_matches('/'),
        path
    )
}

pub fn timeout_duration() -> Duration {
    Duration::from_millis(Config::get().im_service_request_timeout_ms.max(300) as u64)
}

pub async fn send_json_without_service_token<T: DeserializeOwned>(
    req: RequestBuilder,
) -> Result<T, String> {
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

#[allow(dead_code)]
pub async fn send_json<T: DeserializeOwned>(req: RequestBuilder) -> Result<T, String> {
    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

pub async fn send_json_with_service_token<T: DeserializeOwned>(
    req: RequestBuilder,
) -> Result<T, String> {
    let resp = apply_service_auth(req).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

fn apply_auth(req: RequestBuilder) -> RequestBuilder {
    if let Some(access_token) = current_access_token() {
        return req.bearer_auth(access_token);
    }
    apply_service_auth(req)
}

fn apply_service_auth(req: RequestBuilder) -> RequestBuilder {
    let token = Config::get().im_service_service_token.trim();
    if token.is_empty() {
        req
    } else {
        req.header("X-Service-Token", token)
    }
}
