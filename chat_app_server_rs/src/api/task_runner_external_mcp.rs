// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use bytes::BytesMut;
use futures::StreamExt;
use reqwest::Method;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;

use crate::config::Config;
use crate::services::access_token_scope;

static TASK_RUNNER_EXTERNAL_MCP_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

const TASK_RUNNER_EXTERNAL_MCP_RESPONSE_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const TASK_RUNNER_EXTERNAL_MCP_ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/task-runner/external-mcp-configs",
            get(list_external_mcp_configs).post(create_external_mcp_config),
        )
        .route(
            "/api/task-runner/external-mcp-configs/:id",
            get(get_external_mcp_config)
                .patch(update_external_mcp_config)
                .delete(delete_external_mcp_config),
        )
}

async fn list_external_mcp_configs() -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    task_runner_json::<()>(Method::GET, "/api/external-mcp-configs", None)
        .await
        .map(Json)
}

async fn create_external_mcp_config(
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let value = task_runner_json(Method::POST, "/api/external-mcp-configs", Some(&payload)).await?;
    Ok((StatusCode::CREATED, Json(value)))
}

async fn get_external_mcp_config(
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/external-mcp-configs/{}",
        urlencoding::encode(id.trim())
    );
    task_runner_json::<()>(Method::GET, path.as_str(), None)
        .await
        .map(Json)
}

async fn update_external_mcp_config(
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/external-mcp-configs/{}",
        urlencoding::encode(id.trim())
    );
    task_runner_json(Method::PATCH, path.as_str(), Some(&payload))
        .await
        .map(Json)
}

async fn delete_external_mcp_config(
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/external-mcp-configs/{}",
        urlencoding::encode(id.trim())
    );
    task_runner_empty::<()>(Method::DELETE, path.as_str(), None).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn task_runner_json<T: Serialize + ?Sized>(
    method: Method,
    path: &str,
    body: Option<&T>,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let response = task_runner_request(method, path, body).await?;
    let body = read_task_runner_external_mcp_body_limited(
        response,
        TASK_RUNNER_EXTERNAL_MCP_RESPONSE_LIMIT_BYTES,
    )
    .await
    .map_err(|err| bad_gateway("读取任务系统响应失败", err))?;
    serde_json::from_slice::<Value>(body.as_ref())
        .map_err(|err| bad_gateway("解析任务系统响应失败", err.to_string()))
}

async fn task_runner_empty<T: Serialize + ?Sized>(
    method: Method,
    path: &str,
    body: Option<&T>,
) -> Result<(), (StatusCode, Json<Value>)> {
    let _response = task_runner_request(method, path, body).await?;
    Ok(())
}

async fn task_runner_request<T: Serialize + ?Sized>(
    method: Method,
    path: &str,
    body: Option<&T>,
) -> Result<reqwest::Response, (StatusCode, Json<Value>)> {
    let cfg = Config::try_get().map_err(|err| internal_error("读取任务系统配置失败", err))?;
    let base_url = cfg.task_runner_base_url.trim().trim_end_matches('/');
    if base_url.is_empty() {
        return Err(internal_error(
            "任务系统地址未配置",
            "CHATOS_TASK_RUNNER_BASE_URL 为空".to_string(),
        ));
    }
    let token = access_token_scope::get_current_access_token().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "当前登录态缺少真实用户 token"})),
        )
    })?;
    let endpoint = format!("{base_url}{path}");
    let timeout_ms = cfg.task_runner_request_timeout_ms.max(300) as u64;
    let method_label = method.as_str().to_string();
    let mut request = task_runner_external_mcp_http_client()
        .request(method, &endpoint)
        .bearer_auth(token)
        .timeout(Duration::from_millis(timeout_ms));
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|err| {
        tracing::warn!(
            error = %err,
            upstream = %endpoint,
            method = %method_label,
            timeout_ms = timeout_ms,
            "task_runner_external_mcp.forward_failed"
        );
        task_runner_forward_error("Chatos 后端转发任务系统失败", &endpoint, timeout_ms, &err)
    })?;
    if response.status().is_success() {
        return Ok(response);
    }
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let text = read_task_runner_external_mcp_body_limited(
        response,
        TASK_RUNNER_EXTERNAL_MCP_ERROR_BODY_PREVIEW_BYTES,
    )
    .await
    .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
    .unwrap_or_else(|err| err);
    let payload = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| {
        json!({
            "error": if text.trim().is_empty() { "任务系统请求失败" } else { text.trim() }
        })
    });
    Err((status, Json(payload)))
}

fn task_runner_external_mcp_http_client() -> &'static reqwest::Client {
    TASK_RUNNER_EXTERNAL_MCP_HTTP_CLIENT.get_or_init(reqwest::Client::new)
}

async fn read_task_runner_external_mcp_body_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<bytes::Bytes, String> {
    if let Some(content_length) = response.content_length() {
        ensure_task_runner_external_mcp_body_within_limit(content_length as usize, limit_bytes)?;
    }

    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let next_len = body.len().saturating_add(chunk.len());
        ensure_task_runner_external_mcp_body_within_limit(next_len, limit_bytes)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body.freeze())
}

fn ensure_task_runner_external_mcp_body_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "Task Runner external MCP response exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

fn internal_error(message: &str, detail: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": message,
            "detail": detail,
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::ensure_task_runner_external_mcp_body_within_limit;

    #[test]
    fn task_runner_external_mcp_body_limit_accepts_boundary_size() {
        assert!(ensure_task_runner_external_mcp_body_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn task_runner_external_mcp_body_limit_rejects_oversized_body() {
        let err = ensure_task_runner_external_mcp_body_within_limit(1025, 1024)
            .expect_err("oversized body should fail");

        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}

fn bad_gateway(message: &str, detail: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({
            "error": message,
            "detail": detail,
        })),
    )
}

fn task_runner_forward_error(
    message: &str,
    endpoint: &str,
    timeout_ms: u64,
    err: &reqwest::Error,
) -> (StatusCode, Json<Value>) {
    let (status, detail) = if err.is_timeout() {
        (
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                "请求任务系统超时（{}ms）：{}。如果正在新增或更新外部 MCP，任务系统会先执行 tools/list 连通性测试；这表示请求没有在 Chatos 等待时间内完成，不代表 task_runner_service backend 没启动。source={err}",
                timeout_ms, endpoint
            ),
        )
    } else if err.is_connect() {
        (
            StatusCode::BAD_GATEWAY,
            format!(
                "无法连接任务系统：{}。请确认 CHATOS_TASK_RUNNER_BASE_URL / TASK_RUNNER_BASE_URL 指向正在运行的 task_runner_service backend。source={err}",
                endpoint
            ),
        )
    } else {
        (
            StatusCode::BAD_GATEWAY,
            format!("转发任务系统请求失败：{}。source={err}", endpoint),
        )
    };
    (
        status,
        Json(json!({
            "error": message,
            "detail": detail,
        })),
    )
}
