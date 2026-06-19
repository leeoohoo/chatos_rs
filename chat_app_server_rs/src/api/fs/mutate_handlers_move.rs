use std::fs;

use axum::Json;
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::core::auth::AuthUser;
use crate::services::code_nav::symbol_index::invalidate_project_symbol_indexes_for_path;
use crate::services::project_fs_cache::invalidate_directory_listing_cache_for_path;
use crate::services::workspace_realtime_watcher::{
    note_workspace_path_changed, suppress_logged_path,
};

use super::super::contracts::FsMoveRequest;
use super::super::helpers::is_valid_entry_name;
use super::super::policy::FsPathPolicy;
use super::policy_error_tuple;

pub(in super::super) async fn move_entry(
    auth: AuthUser,
    Json(req): Json<FsMoveRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let source_raw = req
        .source_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(source_raw) = source_raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "源路径不能为空" })),
        );
    };

    let target_parent_raw = req
        .target_parent_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(target_parent_raw) = target_parent_raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目标目录不能为空" })),
        );
    };

    let source_path =
        match policy.authorize_existing_entry(source_raw.as_str(), "源路径不存在", "源路径不合法")
        {
            Ok(value) => value,
            Err(err) => return policy_error_tuple(err),
        };
    if let Err(err) = policy.require_write(&source_path) {
        return policy_error_tuple(err);
    }
    if let Err(err) = policy.forbid_root_mutation(source_path.path.as_path()) {
        return policy_error_tuple(err);
    }

    let target_parent = match policy.authorize_existing_dir(
        target_parent_raw.as_str(),
        "目标目录不存在",
        "目标路径不是目录",
    ) {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    if let Err(err) = policy.require_write(&target_parent) {
        return policy_error_tuple(err);
    }

    let source_name = source_path
        .path
        .file_name()
        .and_then(|v| v.to_str())
        .map(|v| v.to_string())
        .filter(|v| !v.is_empty());
    let Some(source_name) = source_name else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "源路径名称不合法" })),
        );
    };

    let target_name = req
        .target_name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(source_name);
    if !is_valid_entry_name(&target_name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目标名称不合法" })),
        );
    }

    let target_path = target_parent.path.join(&target_name);
    if let Err(err) = policy.forbid_root_mutation(target_path.as_path()) {
        return policy_error_tuple(err);
    }
    let source_norm = source_path.path.to_string_lossy().to_string();
    let target_norm = target_path.to_string_lossy().to_string();
    if source_norm == target_norm {
        return (
            StatusCode::OK,
            Json(json!({
                "from_path": source_norm,
                "to_path": target_norm,
                "replaced": false,
                "moved": false
            })),
        );
    }

    let source_meta = match fs::symlink_metadata(&source_path.path) {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": err.to_string() })),
            );
        }
    };
    let source_is_dir = source_meta.is_dir() && !source_meta.file_type().is_symlink();

    if source_is_dir {
        let source_canonical = match source_path.path.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": err.to_string() })),
                );
            }
        };
        let target_parent_canonical = match target_parent.path.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": err.to_string() })),
                );
            }
        };
        if target_parent_canonical.starts_with(&source_canonical) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "不支持把目录移动到其子目录中" })),
            );
        }
    }

    let replace_existing = req.replace_existing.unwrap_or(false);
    let mut replaced = false;
    if target_path.exists() {
        if !replace_existing {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "目标目录已存在同名文件或目录" })),
            );
        }
        let target_meta = match fs::symlink_metadata(&target_path) {
            Ok(value) => value,
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": err.to_string() })),
                );
            }
        };
        let remove_result = if target_meta.is_dir() && !target_meta.file_type().is_symlink() {
            fs::remove_dir_all(&target_path)
        } else {
            fs::remove_file(&target_path)
        };
        if let Err(err) = remove_result {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("覆盖目标失败: {}", err) })),
            );
        }
        invalidate_project_symbol_indexes_for_path(target_path.as_path());
        if let Some(project_root) = target_parent.project_root.as_ref() {
            let _ = invalidate_directory_listing_cache_for_path(
                project_root.to_string_lossy().as_ref(),
                target_path.as_path(),
            );
        }
        replaced = true;
    }

    if let Err(err) = fs::rename(&source_path.path, &target_path) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        );
    }
    invalidate_project_symbol_indexes_for_path(source_path.path.as_path());
    invalidate_project_symbol_indexes_for_path(target_path.as_path());
    if let Some(project_root) = source_path.project_root.as_ref() {
        let _ = invalidate_directory_listing_cache_for_path(
            project_root.to_string_lossy().as_ref(),
            source_path.path.as_path(),
        );
    }
    if let Some(project_root) = target_parent.project_root.as_ref() {
        let _ = invalidate_directory_listing_cache_for_path(
            project_root.to_string_lossy().as_ref(),
            target_path.as_path(),
        );
    }
    suppress_logged_path(source_norm.as_str());
    note_workspace_path_changed(source_norm.as_str());
    suppress_logged_path(target_norm.as_str());
    note_workspace_path_changed(target_norm.as_str());

    (
        StatusCode::OK,
        Json(json!({
            "from_path": source_norm,
            "to_path": target_norm,
            "name": target_name,
            "replaced": replaced,
            "is_dir": source_is_dir,
            "moved": true
        })),
    )
}
