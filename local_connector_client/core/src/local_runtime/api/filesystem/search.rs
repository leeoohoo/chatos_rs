// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::workspace::paths::relative_to_workspace;
use crate::LocalRuntime;

use super::super::error::LocalRuntimeApiError;
use super::super::workspace_path::resolve_local_workspace_path;

const MAX_VISITED_FILES: usize = 10_000;
const MAX_SEARCH_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Deserialize)]
pub(super) struct SearchQuery {
    path: String,
    q: String,
    limit: Option<usize>,
    case_sensitive: Option<bool>,
    whole_word: Option<bool>,
}

pub(super) async fn search_entries(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, query.path.as_str(), false).await?;
    let needle = query.q.trim().to_string();
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let search_root = resolved.clone();
    let (entries, visited, truncated) = tokio::task::spawn_blocking(move || {
        let mut matches = Vec::new();
        let mut visited = 0usize;
        walk_files(search_root.path.as_path(), |path, metadata| {
            visited += 1;
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !needle.is_empty()
                && name
                    .to_ascii_lowercase()
                    .contains(needle.to_ascii_lowercase().as_str())
            {
                let relative = relative_to_workspace(&search_root.workspace, path);
                matches.push(json!({
                    "name": name,
                    "path": search_root.logical_child(relative.as_str()),
                    "display_path": search_root.logical_child(relative.as_str()),
                    "is_dir": metadata.is_dir(),
                    "size": metadata.is_file().then_some(metadata.len()),
                }));
            }
            matches.len() < limit && visited < MAX_VISITED_FILES
        })?;
        let truncated = matches.len() >= limit || visited >= MAX_VISITED_FILES;
        Ok::<_, String>((matches, visited, truncated))
    })
    .await
    .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))?
    .map_err(|error| LocalRuntimeApiError::bad_request("local_runtime_fs_search_failed", error))?;
    Ok(Json(json!({
        "path": resolved.logical_path(),
        "query": query.q,
        "entries": entries,
        "visited_dirs": visited,
        "truncated": truncated,
    })))
}

pub(super) async fn search_content(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, query.path.as_str(), false).await?;
    let needle = query.q.clone();
    if needle.is_empty() {
        return Ok(Json(json!({
            "path": resolved.logical_path(),
            "query": needle,
            "entries": [],
            "visited_dirs": 0,
            "truncated": false,
        })));
    }
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let case_sensitive = query.case_sensitive.unwrap_or(false);
    let whole_word = query.whole_word.unwrap_or(false);
    let search_root = resolved.clone();
    let (entries, visited, truncated) = tokio::task::spawn_blocking(move || {
        let mut matches = Vec::new();
        let mut visited = 0usize;
        walk_files(search_root.path.as_path(), |path, metadata| {
            if !metadata.is_file() || metadata.len() > MAX_SEARCH_FILE_BYTES {
                return true;
            }
            visited += 1;
            let Ok(content) = fs::read_to_string(path) else {
                return visited < MAX_VISITED_FILES;
            };
            let relative = relative_to_workspace(&search_root.workspace, path);
            for (line_index, line) in content.lines().enumerate() {
                let Some(column) = find_match(line, needle.as_str(), case_sensitive, whole_word)
                else {
                    continue;
                };
                matches.push(json!({
                    "path": search_root.logical_child(relative.as_str()),
                    "relative_path": relative,
                    "line": line_index + 1,
                    "column": column + 1,
                    "text": line,
                }));
                if matches.len() >= limit {
                    return false;
                }
            }
            visited < MAX_VISITED_FILES
        })?;
        let truncated = matches.len() >= limit || visited >= MAX_VISITED_FILES;
        Ok::<_, String>((matches, visited, truncated))
    })
    .await
    .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))?
    .map_err(|error| LocalRuntimeApiError::bad_request("local_runtime_fs_search_failed", error))?;
    Ok(Json(json!({
        "path": resolved.logical_path(),
        "query": query.q,
        "entries": entries,
        "visited_dirs": visited,
        "truncated": truncated,
    })))
}

fn walk_files(
    root: &Path,
    mut visit: impl FnMut(&Path, &fs::Metadata) -> bool,
) -> Result<(), String> {
    let mut queue = VecDeque::from([PathBuf::from(root)]);
    while let Some(directory) = queue.pop_front() {
        for entry in fs::read_dir(directory.as_path()).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            let path = entry.path();
            if metadata.is_dir() {
                if !ignored_directory(path.as_path()) {
                    queue.push_back(path.clone());
                }
            }
            if !visit(path.as_path(), &metadata) {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn ignored_directory(path: &Path) -> bool {
    matches!(
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        ".git" | ".next" | ".venv" | "build" | "dist" | "node_modules" | "target" | "vendor"
    )
}

fn find_match(line: &str, needle: &str, case_sensitive: bool, whole_word: bool) -> Option<usize> {
    let (haystack, needle) = if case_sensitive {
        (line.to_string(), needle.to_string())
    } else {
        (line.to_ascii_lowercase(), needle.to_ascii_lowercase())
    };
    let mut offset = 0usize;
    while let Some(found) = haystack[offset..].find(needle.as_str()) {
        let start = offset + found;
        let end = start + needle.len();
        if !whole_word || (word_boundary(&haystack, start) && word_boundary(&haystack, end)) {
            return Some(line[..start.min(line.len())].chars().count());
        }
        offset = end.max(start + 1);
        if offset >= haystack.len() {
            break;
        }
    }
    None
}

fn word_boundary(value: &str, index: usize) -> bool {
    if index == 0 || index >= value.len() {
        return true;
    }
    value[..index]
        .chars()
        .next_back()
        .is_none_or(|character| !character.is_alphanumeric() && character != '_')
}
