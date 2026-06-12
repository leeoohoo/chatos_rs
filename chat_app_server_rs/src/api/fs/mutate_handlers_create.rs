use std::fs;
use std::io::Write;

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::code_nav::symbol_index::invalidate_project_symbol_indexes_for_path;
use crate::services::project_fs_cache::invalidate_directory_listing_cache_for_path;
use crate::services::workspace_realtime_watcher::{
    note_workspace_path_changed, suppress_logged_path,
};

use super::super::contracts::{FsCreateFileRequest, FsMkdirRequest, FsWriteFileRequest};
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
    if let Some(project_root) = parent.project_root.as_ref() {
        let _ = invalidate_directory_listing_cache_for_path(
            project_root.to_string_lossy().as_ref(),
            target.as_path(),
        );
    }
    let target_path = target.to_string_lossy().to_string();
    suppress_logged_path(target_path.as_str());
    note_workspace_path_changed(target_path.as_str());

    (
        StatusCode::CREATED,
        Json(json!({
            "path": target_path,
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
            );
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
    if let Some(project_root) = parent.project_root.as_ref() {
        let _ = invalidate_directory_listing_cache_for_path(
            project_root.to_string_lossy().as_ref(),
            target.as_path(),
        );
    }
    let target_path = target.to_string_lossy().to_string();
    suppress_logged_path(target_path.as_str());
    note_workspace_path_changed(target_path.as_str());
    (
        StatusCode::CREATED,
        Json(json!({
            "path": target_path,
            "parent": parent.path.to_string_lossy(),
            "name": name,
            "size": size,
            "created": true
        })),
    )
}

const MAX_WRITE_BYTES: usize = 2 * 1024 * 1024;

pub(in super::super) async fn write_file(
    auth: AuthUser,
    Json(req): Json<FsWriteFileRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let raw = req
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(raw) = raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "文件路径不能为空" })),
        );
    };
    let Some(content) = req.content else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "文件内容不能为空" })),
        );
    };
    if content.len() > MAX_WRITE_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "文件内容过大，无法保存",
                "size": content.len(),
                "limit": MAX_WRITE_BYTES,
            })),
        );
    }

    let authorized =
        match policy.authorize_existing_file(raw.as_str(), "路径不存在", "路径不是文件")
        {
            Ok(value) => value,
            Err(err) => return policy_error_tuple(err),
        };
    if let Err(err) = policy.require_write(&authorized) {
        return policy_error_tuple(err);
    }

    if let Err(err) = fs::write(&authorized.path, content.as_bytes()) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        );
    }

    let meta = match fs::metadata(&authorized.path) {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            );
        }
    };
    invalidate_project_symbol_indexes_for_path(authorized.path.as_path());
    if let Some(project_root) = authorized.project_root.as_ref() {
        let _ = invalidate_directory_listing_cache_for_path(
            project_root.to_string_lossy().as_ref(),
            authorized.path.as_path(),
        );
    }
    let target_path = authorized.path.to_string_lossy().to_string();
    suppress_logged_path(target_path.as_str());
    note_workspace_path_changed(target_path.as_str());

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "path": target_path,
            "name": authorized.path.file_name().and_then(|value| value.to_str()).unwrap_or(""),
            "size": meta.len(),
            "modified_at": meta.modified().ok().and_then(super::super::helpers::format_system_time),
        })),
    )
}
