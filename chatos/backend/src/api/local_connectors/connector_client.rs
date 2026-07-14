// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use axum::http::StatusCode;
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::services::access_token_scope;

use super::root_path::normalize_local_relative_path;
pub(super) async fn connector_get_json<T: DeserializeOwned>(
    path: &str,
    query: &[(&str, String)],
) -> Result<T, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let request = reqwest::Client::new()
        .get(connector_url(cfg, path))
        .bearer_auth(token)
        .query(query)
        .timeout(connector_timeout(cfg));
    send_connector_json(request).await
}

pub(super) async fn connector_post_json<T: DeserializeOwned, B: Serialize + ?Sized>(
    path: &str,
    body: &B,
) -> Result<T, (StatusCode, Json<Value>)> {
    connector_post_json_with_headers(path, body, &[]).await
}

pub(super) async fn connector_put_json<T: DeserializeOwned, B: Serialize + ?Sized>(
    path: &str,
    body: &B,
) -> Result<T, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let request = reqwest::Client::new()
        .put(connector_url(cfg, path))
        .bearer_auth(token)
        .json(body)
        .timeout(connector_timeout(cfg));
    send_connector_json(request).await
}

pub(super) async fn connector_post_json_with_headers<T: DeserializeOwned, B: Serialize + ?Sized>(
    path: &str,
    body: &B,
    headers: &[(&str, String)],
) -> Result<T, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let mut request = reqwest::Client::new()
        .post(connector_url(cfg, path))
        .bearer_auth(token)
        .json(body)
        .timeout(connector_timeout(cfg));
    for (key, value) in headers {
        request = request.header(*key, value.as_str());
    }
    send_connector_json(request).await
}

pub(super) async fn connector_delete_json(path: &str) -> Result<Value, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let request = reqwest::Client::new()
        .delete(connector_url(cfg, path))
        .bearer_auth(token)
        .timeout(connector_timeout(cfg));
    send_connector_json(request).await
}

async fn send_connector_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> Result<T, (StatusCode, Json<Value>)> {
    let response = request
        .send()
        .await
        .map_err(|err| connector_unavailable(err.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| connector_unavailable(err.to_string()))?;
    let value = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str::<Value>(text.as_str()).unwrap_or_else(|_| {
            json!({
                "error": text,
            })
        })
    };
    if !status.is_success() {
        return Err((status, Json(value)));
    }
    serde_json::from_value(value).map_err(|err| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": "Local Connector service 响应格式错误",
                "detail": err.to_string(),
            })),
        )
    })
}

fn current_access_token() -> Result<String, (StatusCode, Json<Value>)> {
    access_token_scope::get_current_access_token().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "当前请求缺少可转发的 access token" })),
        )
    })
}

fn connector_url(cfg: &Config, path: &str) -> String {
    format!(
        "{}{}",
        cfg.local_connector_service_base_url
            .trim()
            .trim_end_matches('/'),
        path
    )
}

pub(super) fn local_connector_mcp_relay_path(
    device_id: &str,
    workspace_id: &str,
    cwd: Option<&str>,
) -> String {
    let mut path = format!(
        "/api/local-connectors/relay/{}/mcp?workspace_id={}",
        urlencoding::encode(device_id),
        urlencoding::encode(workspace_id)
    );
    if let Some(cwd) = cwd.and_then(|value| normalize_local_relative_path(Some(value))) {
        path.push_str("&cwd=");
        path.push_str(urlencoding::encode(cwd.as_str()).as_ref());
    }
    path
}

fn connector_timeout(cfg: &Config) -> Duration {
    Duration::from_millis(cfg.local_connector_service_request_timeout_ms.max(300) as u64)
}

fn connector_unavailable(detail: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({
            "error": "Local Connector service 不可用",
            "detail": detail,
        })),
    )
}
