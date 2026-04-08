use std::time::Duration;

use once_cell::sync::Lazy;
use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;

use crate::config::Config;

use super::{current_access_token, ListResponse};

static MEMORY_SERVER_HTTP: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

pub fn client() -> &'static reqwest::Client {
    &MEMORY_SERVER_HTTP
}

pub fn build_url(path: &str) -> String {
    format!(
        "{}{}",
        Config::get().memory_server_base_url.trim_end_matches('/'),
        path
    )
}

pub fn timeout_duration() -> Duration {
    Duration::from_millis(Config::get().memory_server_request_timeout_ms.max(300) as u64)
}

pub fn context_timeout_duration() -> Duration {
    Duration::from_millis(Config::get().memory_server_context_timeout_ms.max(300) as u64)
}

pub fn push_limit_offset_params(
    params: &mut Vec<(String, String)>,
    limit: Option<i64>,
    offset: i64,
) {
    if let Some(value) = limit {
        params.push(("limit".to_string(), value.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }
}

pub async fn send_delete_result(req: RequestBuilder) -> Result<bool, String> {
    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    Ok(true)
}

pub async fn send_list<T: DeserializeOwned>(
    path: &str,
    params: &[(String, String)],
) -> Result<Vec<T>, String> {
    let req = client()
        .get(build_url(path).as_str())
        .timeout(timeout_duration())
        .query(params);
    let resp: ListResponse<T> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn send_json<T: DeserializeOwned>(req: RequestBuilder) -> Result<T, String> {
    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

pub async fn send_optional_json<T: DeserializeOwned>(
    req: RequestBuilder,
) -> Result<Option<T>, String> {
    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map(Some).map_err(|e| e.to_string())
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

fn apply_auth(req: RequestBuilder) -> RequestBuilder {
    if let Some(access_token) = current_access_token() {
        return req.bearer_auth(access_token);
    }
    let token = Config::get().memory_server_service_token.trim();
    if token.is_empty() {
        req
    } else {
        req.header("X-Service-Token", token)
    }
}
