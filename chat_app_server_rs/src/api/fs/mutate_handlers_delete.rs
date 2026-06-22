use std::fs;

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::code_nav::symbol_index::invalidate_project_symbol_indexes_for_path;
use crate::services::project_fs_cache::invalidate_directory_listing_cache_for_path;
use crate::services::workspace_realtime_watcher::{
    note_workspace_path_changed, suppress_logged_path,
};

use super::super::contracts::FsDeleteRequest;
use super::super::policy::FsPathPolicy;
use super::policy_error_tuple;

pub(in super::super) async fn delete_entry(
    auth: AuthUser,
    Json(req): Json<FsDeleteRequest>,
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
            Json(json!({ "error": "路径不能为空" })),
        );
    };

    let path = match policy.authorize_existing_entry(raw.as_str(), "路径不存在", "路径不合法")
    {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    if let Err(err) = policy.require_write(&path) {
        return policy_error_tuple(err);
    }
    if let Err(err) = policy.forbid_root_mutation(path.path.as_path()) {
        return policy_error_tuple(err);
    }

    let recursive = req.recursive.unwrap_or(false);
    let metadata = match fs::symlink_metadata(&path.path) {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": err.to_string() })),
            );
        }
    };
    let is_symlink = metadata.file_type().is_symlink();
    let is_dir = metadata.is_dir() && !is_symlink;

    let result = if is_dir {
        if recursive {
            fs::remove_dir_all(&path.path)
        } else {
            fs::remove_dir(&path.path)
        }
    } else {
        fs::remove_file(&path.path)
    };

    if let Err(err) = result {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        );
    }
    invalidate_project_symbol_indexes_for_path(path.path.as_path());
    if let Some(project_root) = path.project_root.as_ref() {
        let _ = invalidate_directory_listing_cache_for_path(
            project_root.to_string_lossy().as_ref(),
            path.path.as_path(),
        );
    }
    let deleted_path = path.path.to_string_lossy().to_string();
    suppress_logged_path(deleted_path.as_str());
    note_workspace_path_changed(deleted_path.as_str());

    (
        StatusCode::OK,
        Json(json!({
            "path": deleted_path,
            "is_dir": is_dir,
            "recursive": recursive,
            "deleted": true
        })),
    )
}
