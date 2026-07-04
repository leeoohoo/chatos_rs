// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::auth::AuthUser;
use axum::http::StatusCode;
use axum::{extract::Query, Json};
use serde_json::{json, Value};

use super::super::contracts::FsQuery;
use super::super::helpers::read_dir_entries;
use super::super::policy::FsPathPolicy;
use super::policy_error_tuple;
use crate::services::project_fs_cache::{
    read_cached_directory_listing, write_cached_directory_listing,
};

pub(in super::super) async fn list_dirs(
    auth: AuthUser,
    Query(query): Query<FsQuery>,
) -> (StatusCode, Json<Value>) {
    list_entries_impl(auth, query, false).await
}

pub(in super::super) async fn list_entries(
    auth: AuthUser,
    Query(query): Query<FsQuery>,
) -> (StatusCode, Json<Value>) {
    list_entries_impl(auth, query, true).await
}

async fn list_entries_impl(
    auth: AuthUser,
    query: FsQuery,
    include_files: bool,
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
            StatusCode::OK,
            Json(json!({
                "path": Value::Null,
                "parent": Value::Null,
                "entries": Vec::<Value>::new(),
                "roots": policy.roots_json()
            })),
        );
    };

    let path = match policy.authorize_existing_dir(raw.as_str(), "路径不存在", "路径不是目录")
    {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };

    let force_refresh = query.force_refresh.unwrap_or(false);
    let entries = match load_directory_entries(
        path.project_root.as_ref(),
        &path.path,
        &path.navigation_root,
        include_files,
        force_refresh,
    ) {
        Ok(v) => v,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err })),
            );
        }
    };
    let parent = policy
        .parent_for(&path)
        .map(|parent| policy.display_path(std::path::Path::new(parent.as_str())));

    (
        StatusCode::OK,
        Json(json!({
            "path": policy.display_path(path.path.as_path()),
            "parent": parent,
            "writable": path.can_write,
            "entries": entries,
            "roots": Vec::<Value>::new()
        })),
    )
}

fn load_directory_entries(
    project_root: Option<&std::path::PathBuf>,
    path: &std::path::Path,
    navigation_root: &std::path::Path,
    include_files: bool,
    force_refresh: bool,
) -> Result<Vec<Value>, String> {
    if !force_refresh {
        if let Some(project_root) = project_root {
            if let Some(cached) = read_cached_directory_listing(
                project_root.to_string_lossy().as_ref(),
                path,
                include_files,
            )? {
                return Ok(cached);
            }
        }
    }

    let entries = read_dir_entries(path, navigation_root, include_files)?;
    if let Some(project_root) = project_root {
        let _ = write_cached_directory_listing(
            project_root.to_string_lossy().as_ref(),
            path,
            include_files,
            entries.as_slice(),
        );
    }
    Ok(entries)
}
