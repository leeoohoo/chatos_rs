// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use crate::core::auth::AuthUser;
use axum::http::StatusCode;
use axum::{extract::Query, Json};
use base64::Engine;
use serde_json::{json, Value};

use super::super::contracts::FsReadQuery;
use super::super::helpers::format_system_time;
use super::super::policy::FsPathPolicy;
use super::super::read_mode::should_render_text;
use super::policy_error_tuple;

const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;

pub(in super::super) async fn read_file(
    auth: AuthUser,
    Query(query): Query<FsReadQuery>,
) -> (StatusCode, Json<Value>) {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let raw = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(raw) = raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不能为空" })),
        );
    };
    let path = match policy.authorize_existing_file(raw.as_str(), "路径不存在", "路径不是文件")
    {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };

    let meta = match fs::metadata(&path.path) {
        Ok(m) => m,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            );
        }
    };
    let size = meta.len();
    if size > MAX_PREVIEW_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "文件过大，无法预览",
                "size": size,
                "limit": MAX_PREVIEW_BYTES
            })),
        );
    }

    let bytes = match fs::read(&path.path) {
        Ok(b) => b,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            );
        }
    };

    let mime = mime_guess::from_path(&path.path).first_or_octet_stream();
    let content_type = mime.essence_str().to_string();
    let should_render = should_render_text(&path.path, &bytes, &content_type);

    let (is_binary, content) = if should_render {
        (
            false,
            Value::String(String::from_utf8_lossy(&bytes).to_string()),
        )
    } else {
        (
            true,
            Value::String(base64::engine::general_purpose::STANDARD.encode(&bytes)),
        )
    };

    let modified_at = meta.modified().ok().and_then(format_system_time);

    (
        StatusCode::OK,
        Json(json!({
            "path": policy.display_path(path.path.as_path()),
            "display_path": policy.display_path(path.path.as_path()),
            "name": path.path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
            "size": size,
            "content_type": content_type,
            "is_binary": is_binary,
            "writable": path.can_write,
            "modified_at": modified_at,
            "content": content
        })),
    )
}
