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

pub fn try_build_url(path: &str) -> Result<String, String> {
    Ok(format!(
        "{}{}",
        Config::try_get()?
            .memory_server_base_url
            .trim_end_matches('/'),
        path
    ))
}

pub fn try_timeout_duration() -> Result<Duration, String> {
    Ok(Duration::from_millis(
        Config::try_get()?.memory_server_request_timeout_ms.max(300) as u64,
    ))
}

pub fn try_background_job_timeout_duration() -> Result<Duration, String> {
    Ok(Duration::from_millis(
        Config::try_get()?
            .memory_server_request_timeout_ms
            .max(300)
            .max(120_000) as u64,
    ))
}

pub fn try_context_timeout_duration() -> Result<Duration, String> {
    Ok(Duration::from_millis(
        Config::try_get()?.memory_server_context_timeout_ms.max(300) as u64,
    ))
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
    let resp = try_apply_auth(req)?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        return Err(read_status_detail_error(resp).await);
    }
    Ok(true)
}

pub async fn send_list<T: DeserializeOwned>(
    path: &str,
    params: &[(String, String)],
) -> Result<Vec<T>, String> {
    let req = client()
        .get(try_build_url(path)?)
        .timeout(try_timeout_duration()?)
        .query(params);
    let resp: ListResponse<T> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn send_json<T: DeserializeOwned>(req: RequestBuilder) -> Result<T, String> {
    let resp = try_apply_auth(req)?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(read_status_detail_error(resp).await);
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

pub async fn send_optional_json<T: DeserializeOwned>(
    req: RequestBuilder,
) -> Result<Option<T>, String> {
    let resp = try_apply_auth(req)?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(read_status_detail_error(resp).await);
    }
    resp.json::<T>().await.map(Some).map_err(|e| e.to_string())
}

pub async fn send_json_without_service_token<T: DeserializeOwned>(
    req: RequestBuilder,
) -> Result<T, String> {
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(read_status_detail_error(resp).await);
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

pub(crate) async fn read_status_detail_error(resp: reqwest::Response) -> String {
    let status = resp.status();
    let detail = resp.text().await.unwrap_or_default();
    format_status_detail_error(status, detail.as_str())
}

fn format_status_detail_error(status: reqwest::StatusCode, detail: &str) -> String {
    format!("status={} detail={}", status, detail)
}

pub(crate) fn try_apply_auth(req: RequestBuilder) -> Result<RequestBuilder, String> {
    if let Some(access_token) = current_access_token() {
        return Ok(req.bearer_auth(access_token));
    }
    let token = Config::try_get()?
        .memory_server_service_token
        .trim()
        .to_string();
    if token.is_empty() {
        Ok(req)
    } else {
        Ok(req.header("X-Service-Token", token))
    }
}

#[cfg(test)]
mod tests {
    use super::format_status_detail_error;

    #[test]
    fn formats_status_detail_errors_with_existing_shape() {
        assert_eq!(
            format_status_detail_error(reqwest::StatusCode::BAD_GATEWAY, "upstream failed"),
            "status=502 Bad Gateway detail=upstream failed"
        );
    }
}
