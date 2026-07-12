// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{
    header::{ACCEPT, CONTENT_TYPE},
    HeaderMap, Method, StatusCode, Uri,
};
use axum::response::Response;
use axum::Extension;
use serde_json::Value;

use super::ApiError;
use crate::models::CurrentUser;
use crate::state::AppState;

pub(super) async fn memory_engine_proxy(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(path): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let suffix = normalize_suffix(path.as_str())?;
    validate_request(&method, suffix.as_str(), uri.query(), body.as_ref(), &user)?;
    let secret = state
        .config
        .memory_engine_operator_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::internal("Local Connector Service Memory Engine secret is not configured")
        })?;
    let mut target_url = format!(
        "{}/{}",
        state.config.memory_engine_base_url.trim_end_matches('/'),
        suffix
    );
    if let Some(query) = uri.query().map(str::trim).filter(|value| !value.is_empty()) {
        target_url.push('?');
        target_url.push_str(query);
    }
    let client = reqwest::Client::builder()
        .timeout(state.config.memory_engine_request_timeout)
        .build()
        .map_err(|err| ApiError::internal(format!("build Memory Engine client failed: {err}")))?;
    let scope = if suffix.starts_with("admin/sources") {
        "memory.source"
    } else {
        "memory.data"
    };
    let token = chatos_service_runtime::issue_internal_service_token(
        secret,
        "local-connector-service",
        "memory-engine",
        scope,
        60,
    )
    .map_err(ApiError::internal)?;
    let mut request = client
        .request(method, target_url)
        .header("x-memory-caller", "local-connector-service")
        .header("x-memory-internal-token", token);
    if let Some(content_type) = headers.get(CONTENT_TYPE) {
        request = request.header(CONTENT_TYPE.as_str(), content_type);
    }
    if let Some(accept) = headers.get(ACCEPT) {
        request = request.header(ACCEPT.as_str(), accept);
    }
    if !body.is_empty() {
        request = request.body(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| ApiError::bad_gateway(format!("Memory Engine request failed: {err}")))?;
    let status = StatusCode::from_u16(response.status().as_u16()).map_err(|err| {
        ApiError::bad_gateway(format!("Memory Engine returned invalid status: {err}"))
    })?;
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = response.bytes().await.map_err(|err| {
        ApiError::bad_gateway(format!("read Memory Engine response failed: {err}"))
    })?;
    let mut builder = Response::builder().status(status);
    if let Some(content_type) = content_type {
        builder = builder.header(CONTENT_TYPE, content_type);
    }
    builder.body(Body::from(bytes)).map_err(|err| {
        ApiError::internal(format!("build Memory Engine proxy response failed: {err}"))
    })
}

fn normalize_suffix(path: &str) -> Result<String, ApiError> {
    let path = path.trim().trim_start_matches('/');
    let suffix = path
        .strip_prefix("api/memory-engine/v1/")
        .or_else(|| path.strip_prefix("api/memory-engine/v1"))
        .unwrap_or(path)
        .trim_start_matches('/');
    if suffix.is_empty() {
        return Err(ApiError::bad_request(
            "Memory Engine proxy path is required",
        ));
    }
    Ok(suffix.to_string())
}

fn validate_request(
    method: &Method,
    suffix: &str,
    query: Option<&str>,
    body: &[u8],
    user: &CurrentUser,
) -> Result<(), ApiError> {
    if !path_allowed(method, suffix) {
        return Err(ApiError::forbidden(
            "Memory Engine proxy path is not allowed for Local Connector approval memory",
        ));
    }
    if suffix == "admin/sources/local_connector_approval" {
        return Ok(());
    }
    let parsed_body =
        if body.is_empty() {
            None
        } else {
            Some(serde_json::from_slice::<Value>(body).map_err(|_| {
                ApiError::bad_request("Memory Engine proxy body must be valid JSON")
            })?)
        };
    let tenant_id = query_param(query, "tenant_id")
        .or_else(|| {
            parsed_body
                .as_ref()
                .and_then(|value| json_field(value, "tenant_id"))
        })
        .ok_or_else(|| ApiError::bad_request("Memory Engine proxy tenant_id is required"))?;
    if tenant_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Memory Engine proxy tenant_id does not match current user",
        ));
    }
    if let Some(source_id) = query_param(query, "source_id").or_else(|| {
        parsed_body
            .as_ref()
            .and_then(|value| json_field(value, "source_id"))
    }) {
        if source_id != "local_connector_approval" {
            return Err(ApiError::forbidden(
                "Memory Engine proxy source_id is not allowed",
            ));
        }
    }
    Ok(())
}

fn path_allowed(method: &Method, suffix: &str) -> bool {
    if method == Method::PUT && suffix == "admin/sources/local_connector_approval" {
        return true;
    }
    if method == Method::POST && suffix == "context/compose" {
        return true;
    }
    let parts = suffix.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0] != "threads" || !is_approval_thread(parts[1]) {
        return false;
    }
    matches!(
        (method, parts.as_slice()),
        (&Method::PUT, ["threads", _])
            | (&Method::GET, ["threads", _])
            | (&Method::PUT, ["threads", _, "records", "batch-sync"])
            | (&Method::GET, ["threads", _, "records"])
            | (&Method::GET, ["threads", _, "records", "count"])
            | (&Method::GET, ["threads", _, "compact-turns"])
            | (&Method::GET, ["threads", _, "turns", _, "process-records"])
            | (&Method::POST, ["threads", _, "active-summary", "run"])
            | (&Method::GET, ["threads", _, "active-summary", "status"])
            | (&Method::POST, ["threads", _, "summaries", "run"])
            | (&Method::GET, ["threads", _, "summaries"])
    )
}

fn is_approval_thread(thread_id: &str) -> bool {
    thread_id.starts_with("local_connector_command_approval:")
        || thread_id.starts_with("local_connector_command_approval%3A")
        || thread_id.starts_with("local_connector_command_approval%3a")
}

fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let item_key = parts.next()?.trim();
        let item_value = parts.next().unwrap_or_default().trim();
        (item_key == key && !item_value.is_empty()).then(|| item_value.to_string())
    })
}

fn json_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
