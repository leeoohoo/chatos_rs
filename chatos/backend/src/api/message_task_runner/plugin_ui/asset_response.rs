// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::Body;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_LENGTH, CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY,
};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chatos_plugin_management_sdk::{
    PluginUiAssetKind, PluginUiAssetReadResponse, PluginUiReadyEventPayload, PluginUiSnapshot,
    PLUGIN_UI_ASSET_MAX_BYTES, PLUGIN_UI_ENTRYPOINT_MAX_BYTES, PLUGIN_UI_HOST_CSP_V1,
};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::ApiError;
use crate::core::auth::AuthUser;

pub(super) fn validate_asset_response(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    relative_path: &str,
    response: &PluginUiAssetReadResponse,
) -> Result<(), ApiError> {
    if response.run_id != ready.run_id
        || response.owner_user_id != auth.user_id
        || response.plugin_id != ready.plugin_id
        || response.release_id != ready.release_id
        || response.artifact_sha256 != ready.artifact_sha256
        || response.component_key != ready.component_key
        || response.adapter_session_id != ready.adapter_session_id
        || response.ui_snapshot_sha256 != ready.ui.snapshot_sha256
        || response.relative_path != relative_path
    {
        return Err(bad_gateway("Plugin UI asset session identity 不匹配"));
    }
    let (expected_kind, expected_media_type, expected_size, expected_sha256, max_bytes) =
        if relative_path == ready.ui.relative_source_path {
            (
                PluginUiAssetKind::Entrypoint,
                "text/html; charset=utf-8",
                None,
                ready.ui.content_sha256.as_str(),
                PLUGIN_UI_ENTRYPOINT_MAX_BYTES,
            )
        } else {
            let asset = ready
                .ui
                .assets
                .iter()
                .find(|asset| asset.relative_path == relative_path)
                .ok_or_else(|| not_found("Plugin UI asset 未在 Run snapshot 中声明"))?;
            (
                PluginUiAssetKind::StaticAsset,
                asset.media_type.as_str(),
                Some(asset.size_bytes),
                asset.sha256.as_str(),
                PLUGIN_UI_ASSET_MAX_BYTES,
            )
        };
    if response.kind != expected_kind
        || response.media_type != expected_media_type
        || response.sha256 != expected_sha256
        || expected_size.is_some_and(|size| size != response.size_bytes)
        || response.size_bytes > max_bytes
    {
        return Err(bad_gateway("Plugin UI asset metadata 不匹配"));
    }
    let max_base64_bytes = ((max_bytes as usize).saturating_add(2) / 3).saturating_mul(4);
    if response.body_base64.len() > max_base64_bytes {
        return Err(bad_gateway("Plugin UI asset body 超出限制"));
    }
    let bytes = BASE64_STANDARD
        .decode(response.body_base64.as_bytes())
        .map_err(|_| bad_gateway("Plugin UI asset body 编码无效"))?;
    if bytes.len() as u64 != response.size_bytes
        || hex::encode(Sha256::digest(bytes.as_slice())) != response.sha256
    {
        return Err(bad_gateway("Plugin UI asset body checksum 不匹配"));
    }
    Ok(())
}

pub(super) fn plugin_ui_asset_response(
    ui: &PluginUiSnapshot,
    asset: PluginUiAssetReadResponse,
    parent_origin: Option<&str>,
) -> Result<Response, ApiError> {
    let bytes = BASE64_STANDARD
        .decode(asset.body_base64.as_bytes())
        .map_err(|_| bad_gateway("Plugin UI asset body 编码无效"))?;
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(asset.media_type.as_str())
            .map_err(|_| bad_gateway("Plugin UI asset Content-Type 无效"))?,
    );
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(asset.size_bytes.to_string().as_str())
            .map_err(|_| bad_gateway("Plugin UI asset Content-Length 无效"))?,
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static(
            "camera=(), microphone=(), geolocation=(), display-capture=(), clipboard-read=(), clipboard-write=()",
        ),
    );
    if asset.kind == PluginUiAssetKind::Entrypoint {
        let content_security_policy = plugin_ui_response_content_security_policy(
            ui.content_security_policy.as_str(),
            parent_origin,
        )?;
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(content_security_policy.as_str())
                .map_err(|_| bad_gateway("Plugin UI CSP 无效"))?,
        );
        headers.insert(
            HeaderName::from_static("origin-agent-cluster"),
            HeaderValue::from_static("?1"),
        );
    }
    Ok(response)
}

pub(super) fn plugin_ui_response_content_security_policy(
    immutable_csp: &str,
    parent_origin: Option<&str>,
) -> Result<String, ApiError> {
    if immutable_csp != PLUGIN_UI_HOST_CSP_V1 {
        return Err(bad_gateway("Plugin UI immutable CSP 无效"));
    }
    let Some(parent_origin) = parent_origin else {
        return Ok(immutable_csp.to_string());
    };
    if parent_origin.is_empty()
        || parent_origin.bytes().any(|byte| byte.is_ascii_whitespace())
        || parent_origin.contains(';')
        || parent_origin.contains('\'')
        || parent_origin.contains('"')
    {
        return Err(service_unavailable("Plugin UI parent origin 配置无效"));
    }
    let marker = "frame-ancestors 'self'";
    if immutable_csp.matches(marker).count() != 1 {
        return Err(bad_gateway("Plugin UI frame ancestor policy 无效"));
    }
    Ok(immutable_csp.replace(marker, format!("frame-ancestors {parent_origin}").as_str()))
}

pub(super) fn normalize_requested_asset_path(path: &str) -> Result<String, ApiError> {
    let path = path.trim().trim_start_matches('/');
    if path.is_empty()
        || path.len() > 1024
        || path.contains('\0')
        || path.contains('\\')
        || path.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.len() > 255
        })
    {
        return Err(not_found("Plugin UI asset 路径无效"));
    }
    let relative_path = format!("./{path}");
    if !is_safe_ui_path(relative_path.as_str(), path.ends_with(".html")) {
        return Err(not_found("Plugin UI asset 路径无效"));
    }
    Ok(relative_path)
}

pub(super) fn is_safe_ui_path(path: &str, html: bool) -> bool {
    if !path.starts_with("./ui/")
        || path.len() > 1024
        || path.contains('\0')
        || path.contains('\\')
        || path
            .trim_start_matches("./")
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return false;
    }
    if html {
        path.to_ascii_lowercase().ends_with(".html")
    } else {
        expected_media_type(path).is_some()
    }
}

pub(super) fn expected_media_type(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    match extension.as_str() {
        "js" | "mjs" => Some("text/javascript"),
        "css" => Some("text/css"),
        "json" => Some("application/json"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

pub(super) fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(super) fn map_relay_status(status: reqwest::StatusCode) -> StatusCode {
    StatusCode::from_u16(status.as_u16())
        .ok()
        .filter(|status| {
            matches!(
                *status,
                StatusCode::BAD_REQUEST
                    | StatusCode::FORBIDDEN
                    | StatusCode::NOT_FOUND
                    | StatusCode::PAYLOAD_TOO_LARGE
                    | StatusCode::CONFLICT
                    | StatusCode::GONE
                    | StatusCode::SERVICE_UNAVAILABLE
            )
        })
        .unwrap_or(StatusCode::BAD_GATEWAY)
}

pub(super) fn not_found(message: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({ "error": message })))
}

pub(super) fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message })))
}

pub(super) fn bad_gateway(message: &str) -> ApiError {
    (StatusCode::BAD_GATEWAY, Json(json!({ "error": message })))
}

pub(super) fn service_unavailable(message: &str) -> ApiError {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": message })),
    )
}
