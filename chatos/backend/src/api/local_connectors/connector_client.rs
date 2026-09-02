// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use axum::Json;
use rustls::{ClientConfig, RootCertStore};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use tokio_tungstenite::Connector;

use crate::config::Config;
use crate::services::access_token_scope;

use super::root_path::normalize_local_relative_path;
pub(super) async fn connector_get_json<T: DeserializeOwned>(
    path: &str,
    query: &[(&str, String)],
) -> Result<T, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let request = cfg
        .local_connector_http_client
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

pub(super) async fn connector_post_json_with_timeout<T: DeserializeOwned, B: Serialize + ?Sized>(
    path: &str,
    body: &B,
    timeout: Duration,
) -> Result<T, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let request = connector_client_for_timeout(cfg, timeout)
        .post(connector_url(cfg, path))
        .bearer_auth(token)
        .json(body)
        .timeout(timeout.max(connector_timeout(cfg)));
    send_connector_json(request).await
}

pub(super) async fn connector_put_json<T: DeserializeOwned, B: Serialize + ?Sized>(
    path: &str,
    body: &B,
) -> Result<T, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let request = cfg
        .local_connector_http_client
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
    let timeout = connector_timeout(Config::get());
    connector_post_json_with_headers_and_timeout(path, body, headers, timeout).await
}

pub(super) async fn connector_post_json_with_headers_and_timeout<
    T: DeserializeOwned,
    B: Serialize + ?Sized,
>(
    path: &str,
    body: &B,
    headers: &[(&str, String)],
    timeout: Duration,
) -> Result<T, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let mut request = connector_client_for_timeout(cfg, timeout)
        .post(connector_url(cfg, path))
        .bearer_auth(token)
        .json(body)
        .timeout(timeout.max(connector_timeout(cfg)));
    for (key, value) in headers {
        request = request.header(*key, value.as_str());
    }
    send_connector_json(request).await
}

pub(super) async fn connector_delete_json(path: &str) -> Result<Value, (StatusCode, Json<Value>)> {
    let token = current_access_token()?;
    let cfg = Config::get();
    let request = cfg
        .local_connector_http_client
        .delete(connector_url(cfg, path))
        .bearer_auth(token)
        .timeout(connector_timeout(cfg));
    send_connector_json(request).await
}

async fn send_connector_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> Result<T, (StatusCode, Json<Value>)> {
    let response = crate::core::trace_context::inject_current_trace_context(request)
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

pub(crate) fn local_connector_websocket_url(path: &str) -> String {
    let base = Config::get()
        .local_connector_service_base_url
        .trim()
        .trim_end_matches('/');
    let base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_string()
    };
    format!("{base}{path}")
}

pub(crate) fn local_connector_tls_connector() -> Result<Connector, String> {
    let cfg = Config::get();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let ca_pem = std::fs::read(cfg.local_connector_mtls_ca_cert_path.as_path())
        .map_err(|err| format!("读取 Local Connector mTLS CA 失败: {err}"))?;
    let mut roots = RootCertStore::empty();
    for certificate in rustls_pemfile::certs(&mut BufReader::new(ca_pem.as_slice())) {
        roots
            .add(certificate.map_err(|err| format!("解析 Local Connector mTLS CA 失败: {err}"))?)
            .map_err(|err| format!("加载 Local Connector mTLS CA 失败: {err}"))?;
    }
    if roots.is_empty() {
        return Err("Local Connector mTLS CA 不包含证书".to_string());
    }

    let identity_pem = std::fs::read(cfg.local_connector_mtls_client_identity_path.as_path())
        .map_err(|err| format!("读取 Local Connector mTLS 客户端身份失败: {err}"))?;
    let certificates = rustls_pemfile::certs(&mut BufReader::new(identity_pem.as_slice()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("解析 Local Connector mTLS 客户端证书失败: {err}"))?;
    if certificates.is_empty() {
        return Err("Local Connector mTLS 客户端身份不包含证书".to_string());
    }
    let private_key = rustls_pemfile::private_key(&mut BufReader::new(identity_pem.as_slice()))
        .map_err(|err| format!("解析 Local Connector mTLS 客户端私钥失败: {err}"))?
        .ok_or_else(|| "Local Connector mTLS 客户端身份不包含私钥".to_string())?;
    let tls = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(certificates, private_key)
        .map_err(|err| format!("构建 Local Connector mTLS WebSocket 客户端失败: {err}"))?;
    Ok(Connector::Rustls(Arc::new(tls)))
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

fn connector_client_for_timeout(cfg: &Config, timeout: Duration) -> &reqwest::Client {
    if timeout > connector_timeout(cfg) {
        &cfg.local_connector_long_running_http_client
    } else {
        &cfg.local_connector_http_client
    }
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
