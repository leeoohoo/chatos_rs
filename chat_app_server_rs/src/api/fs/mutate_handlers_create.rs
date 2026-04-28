use std::fs;
use std::io::Write;

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::code_nav::symbol_index::invalidate_project_symbol_indexes_for_path;

use super::super::contracts::{FsCreateFileRequest, FsMkdirRequest};
use super::super::helpers::is_valid_entry_name;
use super::super::policy::FsPathPolicy;
use super::policy_error_tuple;

pub(in super::super) async fn create_dir(
    auth: AuthUser,
    Json(req): Json<FsMkdirRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let parent_raw = req
        .parent_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(parent_raw) = parent_raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "父目录不能为空" })),
        );
    };

    let name_raw = req
        .name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(name) = name_raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目录名称不能为空" })),
        );
    };
    if !is_valid_entry_name(&name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目录名称不合法" })),
        );
    }

    let parent =
        match policy.authorize_existing_dir(parent_raw.as_str(), "父目录不存在", "父路径不是目录")
        {
            Ok(value) => value,
            Err(err) => return policy_error_tuple(err),
        };
    if let Err(err) = policy.require_write(&parent) {
        return policy_error_tuple(err);
    }

    let target = parent.path.join(&name);
    if target.exists() {
        if target.is_dir() {
            return (
                StatusCode::OK,
                Json(json!({
                    "path": target.to_string_lossy(),
                    "parent": parent.path.to_string_lossy(),
                    "name": name,
                    "created": false
                })),
            );
        }
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "同名文件已存在" })),
        );
    }

    if let Err(err) = fs::create_dir(&target) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        );
    }
    invalidate_project_symbol_indexes_for_path(target.as_path());

    (
        StatusCode::CREATED,
        Json(json!({
            "path": target.to_string_lossy(),
            "parent": parent.path.to_string_lossy(),
            "name": name,
            "created": true
        })),
    )
}

pub(in super::super) async fn create_file(
    auth: AuthUser,
    Json(req): Json<FsCreateFileRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let parent_raw = req
        .parent_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(parent_raw) = parent_raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "父目录不能为空" })),
        );
    };

    let name_raw = req
        .name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(name) = name_raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "文件名称不能为空" })),
        );
    };
    if !is_valid_entry_name(&name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "文件名称不合法" })),
        );
    }

    let parent =
        match policy.authorize_existing_dir(parent_raw.as_str(), "父目录不存在", "父路径不是目录")
        {
            Ok(value) => value,
            Err(err) => return policy_error_tuple(err),
        };
    if let Err(err) = policy.require_write(&parent) {
        return policy_error_tuple(err);
    }

    let target = parent.path.join(&name);
    if target.exists() {
        if target.is_file() {
            let size = fs::metadata(&target).map(|meta| meta.len()).unwrap_or(0);
            return (
                StatusCode::OK,
                Json(json!({
                    "path": target.to_string_lossy(),
                    "parent": parent.path.to_string_lossy(),
                    "name": name,
                    "size": size,
                    "created": false
                })),
            );
        }
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "同名目录已存在" })),
        );
    }

    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
    {
        Ok(file) => file,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            )
        }
    };

    if let Some(content) = req.content {
        if let Err(err) = file.write_all(content.as_bytes()) {
            let _ = fs::remove_file(&target);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            );
        }
    }

    let size = fs::metadata(&target).map(|meta| meta.len()).unwrap_or(0);
    invalidate_project_symbol_indexes_for_path(target.as_path());
    (
        StatusCode::CREATED,
        Json(json!({
            "path": target.to_string_lossy(),
            "parent": parent.path.to_string_lossy(),
            "name": name,
            "size": size,
            "created": true
        })),
    )
}
