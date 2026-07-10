// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::core::auth::AuthUser;
use axum::http::StatusCode;
use axum::{extract::Query, Json};
use serde_json::{json, Value};

use super::super::contracts::{FsContentSearchQuery, FsSearchQuery};
use super::super::helpers::format_system_time;
use super::super::policy::FsPathPolicy;
use super::super::search::{is_search_match, normalize_search_keyword};
use super::policy_error_tuple;
use crate::core::user_visible_path::display_path;
use crate::services::workspace_search::{
    search_text as search_workspace_text, TextSearchRequest, DEFAULT_MAX_FILE_BYTES,
    DEFAULT_MAX_VISITS,
};

const DEFAULT_SEARCH_LIMIT: usize = 200;
const MAX_SEARCH_LIMIT: usize = 500;
const MAX_SEARCH_VISITS: usize = 20_000;
const DEFAULT_FS_SEARCH_DEADLINE: Duration = Duration::from_secs(3);

#[derive(Debug)]
struct FsEntrySearchResult {
    entries: Vec<Value>,
    truncated: bool,
    visited_dirs: usize,
}

pub(in super::super) async fn search_entries(
    auth: AuthUser,
    Query(query): Query<FsSearchQuery>,
) -> (StatusCode, Json<Value>) {
    let raw_path = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(raw_path) = raw_path else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "搜索路径不能为空" })),
        );
    };

    let raw_keyword = query
        .q
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(raw_keyword) = raw_keyword else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "搜索关键字不能为空" })),
        );
    };

    let limit = query
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);
    if let Some(response) = super::super::harness_project_bridge::search_entries(
        &auth,
        raw_path.as_str(),
        raw_keyword.as_str(),
        limit,
    )
    .await
    {
        return response;
    }
    if let Some(response) = super::super::local_connector_bridge::search_entries(
        raw_path.as_str(),
        raw_keyword.as_str(),
        limit,
    )
    .await
    {
        return response;
    }

    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let path = match policy.authorize_existing_dir(raw_path.as_str(), "路径不存在", "路径不是目录")
    {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };

    let keyword = normalize_search_keyword(&raw_keyword);
    let root = path.path.clone();
    let root_display = policy.display_path(root.as_path());

    let result = match tokio::task::spawn_blocking({
        let root = root.clone();
        let keyword = keyword.clone();
        move || search_entries_sync(root, keyword, limit, DEFAULT_FS_SEARCH_DEADLINE)
    })
    .await
    {
        Ok(result) => result,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("search task failed: {err}") })),
            );
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "path": root_display,
            "query": keyword,
            "entries": result.entries,
            "truncated": result.truncated,
            "visited_dirs": result.visited_dirs
        })),
    )
}

fn search_entries_sync(
    root: PathBuf,
    keyword: String,
    limit: usize,
    deadline: Duration,
) -> FsEntrySearchResult {
    let started_at = Instant::now();
    let mut stack = vec![root.clone()];
    let mut entries: Vec<Value> = Vec::new();
    let mut visited_dirs = 0usize;
    let mut truncated = false;

    while let Some(dir_path) = stack.pop() {
        if started_at.elapsed() >= deadline {
            truncated = true;
            break;
        }
        if visited_dirs >= MAX_SEARCH_VISITS {
            truncated = true;
            break;
        }
        visited_dirs += 1;

        let iter = match fs::read_dir(&dir_path) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for entry in iter {
            if started_at.elapsed() >= deadline {
                truncated = true;
                break;
            }
            if entries.len() >= limit {
                truncated = true;
                break;
            }

            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };
            let meta = match entry.metadata() {
                Ok(v) => v,
                Err(_) => continue,
            };

            let full_path = entry.path();
            if file_type.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(full_path);
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            let relative_path = full_path
                .strip_prefix(&root)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| full_path.to_string_lossy().to_string());

            if !is_search_match(&name, &relative_path, &keyword) {
                continue;
            }

            let modified_at = meta.modified().ok().and_then(format_system_time);
            entries.push(json!({
                "name": name,
                "path": display_path(full_path.to_string_lossy().as_ref()),
                "display_path": display_path(full_path.to_string_lossy().as_ref()),
                "relative_path": relative_path,
                "is_dir": false,
                "size": Some(meta.len()),
                "modified_at": modified_at
            }));
        }

        if truncated {
            break;
        }
    }

    entries.sort_by(|a, b| {
        let ap = a
            .get("relative_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let bp = b
            .get("relative_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        ap.cmp(&bp)
    });

    FsEntrySearchResult {
        entries,
        truncated,
        visited_dirs,
    }
}

pub(in super::super) async fn search_content(
    auth: AuthUser,
    Query(query): Query<FsContentSearchQuery>,
) -> (StatusCode, Json<Value>) {
    let raw_path = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(raw_path) = raw_path else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "搜索路径不能为空" })),
        );
    };

    let raw_keyword = query
        .q
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(raw_keyword) = raw_keyword else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "搜索关键字不能为空" })),
        );
    };

    let limit = query
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);
    if let Some(response) = super::super::harness_project_bridge::search_content(
        &auth,
        raw_path.as_str(),
        raw_keyword.as_str(),
        limit,
    )
    .await
    {
        return response;
    }
    if let Some(response) = super::super::local_connector_bridge::search_content(
        raw_path.as_str(),
        raw_keyword.as_str(),
        limit,
    )
    .await
    {
        return response;
    }

    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let path = match policy.authorize_existing_dir(raw_path.as_str(), "路径不存在", "路径不是目录")
    {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let query_text = raw_keyword;

    let search_result = tokio::task::spawn_blocking({
        let root = path.path.clone();
        let query_text = query_text.clone();
        let case_sensitive = query.case_sensitive.unwrap_or(false);
        let whole_word = query.whole_word.unwrap_or(false);
        move || {
            search_workspace_text(&TextSearchRequest {
                root,
                query: query_text,
                max_results: limit,
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
                max_visits: DEFAULT_MAX_VISITS,
                case_sensitive,
                whole_word,
                deadline: None,
            })
        }
    })
    .await;
    let result = match search_result {
        Ok(result) => result,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("search task failed: {err}") })),
            );
        }
    };

    match result {
        Ok(mut result) => {
            for entry in &mut result.entries {
                entry.path = display_path(entry.path.as_str());
            }
            (
                StatusCode::OK,
                Json(json!({
                    "path": policy.display_path(path.path.as_path()),
                    "query": query_text,
                    "entries": result.entries,
                    "truncated": result.truncated,
                    "visited_dirs": result.visited_dirs
                })),
            )
        }
        Err(message) if message == "路径不存在" || message == "路径不是目录" => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message })))
        }
        Err(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": message })),
        ),
    }
}
