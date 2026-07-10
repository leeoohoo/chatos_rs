// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};
use url::Url;

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::models::project::harness_project_root_path;
use crate::services::project_management_api_client;

use super::response::{body_download_response, json_error_response};

const HARNESS_PROJECT_SCHEME: &str = "harness";
const HARNESS_PROJECT_HOST: &str = "project";
const MAX_LIST_ENTRIES: usize = 1000;
const MAX_SEARCH_VISITS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct HarnessProjectPath {
    project_id: String,
    relative_path: String,
}

pub(super) fn is_harness_project_path(raw_path: &str) -> bool {
    parse_harness_project_path(raw_path).is_some()
}

pub(super) async fn list_entries(
    auth: &AuthUser,
    raw_path: &str,
    include_files: bool,
) -> Option<(StatusCode, Json<Value>)> {
    let path = parse_harness_project_path(raw_path)?;
    Some(
        match call_harness_tool(
            auth,
            &path,
            "list_dir",
            json!({
                "path": harness_relative_arg(&path),
                "max_entries": MAX_LIST_ENTRIES,
            }),
        )
        .await
        {
            Ok(value) => list_response(&path, value, include_files),
            Err(err) => err,
        },
    )
}

pub(super) async fn read_file(
    auth: &AuthUser,
    raw_path: &str,
) -> Option<(StatusCode, Json<Value>)> {
    let path = parse_harness_project_path(raw_path)?;
    Some(
        match call_harness_tool(
            auth,
            &path,
            "read_file_raw",
            json!({
                "path": harness_relative_arg(&path),
                "with_line_numbers": false,
            }),
        )
        .await
        {
            Ok(value) => read_response(&path, value),
            Err(err) => err,
        },
    )
}

pub(super) async fn search_entries(
    auth: &AuthUser,
    raw_path: &str,
    query: &str,
    limit: usize,
) -> Option<(StatusCode, Json<Value>)> {
    let path = parse_harness_project_path(raw_path)?;
    Some(
        match find_harness_entries(auth, &path, query, limit).await {
            Ok(value) => search_entries_response(&path, value, query),
            Err(err) => err,
        },
    )
}

pub(super) async fn search_content(
    auth: &AuthUser,
    raw_path: &str,
    query: &str,
    limit: usize,
) -> Option<(StatusCode, Json<Value>)> {
    let path = parse_harness_project_path(raw_path)?;
    Some(
        match call_harness_tool(
            auth,
            &path,
            "search_text",
            json!({
                "path": harness_relative_arg(&path),
                "pattern": query,
                "max_results": limit,
            }),
        )
        .await
        {
            Ok(value) => search_content_response(&path, value, query),
            Err(err) => err,
        },
    )
}

pub(super) async fn create_dir(
    auth: &AuthUser,
    parent_path: &str,
    name: &str,
) -> Option<(StatusCode, Json<Value>)> {
    let parent = parse_harness_project_path(parent_path)?;
    let relative = child_relative_path(&parent, name);
    let marker_path = format!("{relative}/.gitkeep");
    Some(
        match call_harness_tool(
            auth,
            &parent,
            "write_file",
            json!({ "path": marker_path, "content": "" }),
        )
        .await
        {
            Ok(_) => created_response(&parent.project_id, relative.as_str(), name, true),
            Err(err) => err,
        },
    )
}

pub(super) async fn create_file(
    auth: &AuthUser,
    parent_path: &str,
    name: &str,
    content: &str,
) -> Option<(StatusCode, Json<Value>)> {
    let parent = parse_harness_project_path(parent_path)?;
    let relative = child_relative_path(&parent, name);
    Some(
        match call_harness_tool(
            auth,
            &parent,
            "write_file",
            json!({ "path": relative, "content": content }),
        )
        .await
        {
            Ok(value) => mutation_response(&parent.project_id, value, name, true),
            Err(err) => err,
        },
    )
}

pub(super) async fn write_file(
    auth: &AuthUser,
    raw_path: &str,
    content: &str,
) -> Option<(StatusCode, Json<Value>)> {
    let path = parse_harness_project_path(raw_path)?;
    let name = path
        .relative_path
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("");
    Some(
        match call_harness_tool(
            auth,
            &path,
            "write_file",
            json!({ "path": harness_relative_arg(&path), "content": content }),
        )
        .await
        {
            Ok(value) => mutation_response(&path.project_id, value, name, false),
            Err(err) => err,
        },
    )
}

pub(super) async fn delete_entry(
    auth: &AuthUser,
    raw_path: &str,
) -> Option<(StatusCode, Json<Value>)> {
    let path = parse_harness_project_path(raw_path)?;
    if path.relative_path.is_empty() {
        return Some((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "不允许删除云端项目根目录" })),
        ));
    }
    Some(
        match call_harness_tool(
            auth,
            &path,
            "delete_path",
            json!({ "path": path.relative_path }),
        )
        .await
        {
            Ok(value) => {
                let result = value.get("result").unwrap_or(&value);
                (
                    StatusCode::OK,
                    Json(json!({
                        "path": logical_path(&path.project_id, path.relative_path.as_str()),
                        "display_path": logical_path(&path.project_id, path.relative_path.as_str()),
                        "deleted": result.get("deleted").and_then(Value::as_bool).unwrap_or(true),
                        "harness_project": true,
                    })),
                )
            }
            Err(err) => err,
        },
    )
}

pub(super) async fn download_entry(auth: &AuthUser, raw_path: &str) -> Option<Response> {
    let path = parse_harness_project_path(raw_path)?;
    if path.relative_path.is_empty() {
        return Some(json_error_response(
            StatusCode::BAD_REQUEST,
            "暂不支持下载整个云端项目目录",
        ));
    }
    Some(
        match call_harness_tool(
            auth,
            &path,
            "read_file_raw",
            json!({
                "path": path.relative_path,
                "with_line_numbers": false,
            }),
        )
        .await
        {
            Ok(value) => {
                let content = value
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let name = path
                    .relative_path
                    .rsplit('/')
                    .find(|part| !part.is_empty())
                    .unwrap_or("download.txt");
                let mime = mime_guess::from_path(name).first_or_text_plain();
                let content_len = content.len() as u64;
                body_download_response(
                    Body::from(content),
                    mime.essence_str(),
                    name,
                    Some(content_len),
                )
            }
            Err((status, body)) => json_error_response(
                status,
                body.0
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("读取 Harness 文件失败"),
            ),
        },
    )
}

async fn call_harness_tool(
    auth: &AuthUser,
    path: &HarnessProjectPath,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let project = ensure_owned_project(path.project_id.as_str(), auth)
        .await
        .map_err(map_project_access_error)?;
    let is_cloud = project
        .source_type
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("cloud"));
    if !is_cloud {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "该项目不是云端项目" })),
        ));
    }
    let cfg = Config::try_get().map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        )
    })?;
    let sync_secret = cfg
        .project_service_sync_secret
        .as_deref()
        .or(cfg.task_runner_callback_secret.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "project service sync secret is not configured" })),
            )
        })?;
    project_management_api_client::call_project_harness_tool(
        cfg.project_service_base_url.as_str(),
        sync_secret,
        path.project_id.as_str(),
        tool_name,
        arguments,
    )
    .await
    .map_err(|err| (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))))
}

async fn find_harness_entries(
    auth: &AuthUser,
    root: &HarnessProjectPath,
    query: &str,
    limit: usize,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let max_results = limit.max(1);
    let query_lower = query.trim().to_lowercase();
    let mut stack = vec![root.relative_path.clone()];
    let mut matches = Vec::new();
    let mut visited_dirs = 0usize;
    while let Some(relative_path) = stack.pop() {
        if matches.len() >= max_results || visited_dirs >= MAX_SEARCH_VISITS {
            break;
        }
        let path = HarnessProjectPath {
            project_id: root.project_id.clone(),
            relative_path: relative_path.clone(),
        };
        let value = call_harness_tool(
            auth,
            &path,
            "list_dir",
            json!({
                "path": harness_relative_arg(&path),
                "max_entries": MAX_LIST_ENTRIES,
            }),
        )
        .await?;
        visited_dirs += 1;
        for entry in value
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if name == ".gitkeep" {
                continue;
            }
            let entry_path = entry
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let is_dir = entry.get("type").and_then(Value::as_str) == Some("dir");
            if name.to_lowercase().contains(query_lower.as_str())
                || entry_path.to_lowercase().contains(query_lower.as_str())
            {
                matches.push(entry.clone());
                if matches.len() >= max_results {
                    break;
                }
            }
            if is_dir {
                stack.push(entry_path.to_string());
            }
        }
    }
    Ok(json!({
        "matches": matches,
        "visited_dirs": visited_dirs,
        "truncated": matches.len() >= max_results || visited_dirs >= MAX_SEARCH_VISITS,
    }))
}

fn list_response(
    path: &HarnessProjectPath,
    value: Value,
    include_files: bool,
) -> (StatusCode, Json<Value>) {
    let mut entries = value
        .get("entries")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| normalize_entry(path.project_id.as_str(), entry, include_files))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sort_entries(entries.as_mut_slice());
    (
        StatusCode::OK,
        Json(json!({
            "path": logical_path(path.project_id.as_str(), path.relative_path.as_str()),
            "display_path": logical_path(path.project_id.as_str(), path.relative_path.as_str()),
            "parent": parent_logical_path(path),
            "writable": true,
            "entries": entries,
            "roots": Vec::<Value>::new(),
            "harness_project": true,
        })),
    )
}

fn normalize_entry(project_id: &str, entry: &Value, include_files: bool) -> Option<Value> {
    let is_dir = entry.get("type").and_then(Value::as_str) == Some("dir");
    if !is_dir && !include_files {
        return None;
    }
    let name = entry
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if name == ".gitkeep" {
        return None;
    }
    let relative_path = entry.get("path").and_then(Value::as_str)?;
    let path = logical_path(project_id, relative_path);
    Some(json!({
        "name": name,
        "path": path,
        "display_path": path,
        "is_dir": is_dir,
        "writable": true,
        "size": entry.get("size").cloned().unwrap_or(Value::Null),
        "modified_at": Value::Null,
    }))
}

fn read_response(path: &HarnessProjectPath, value: Value) -> (StatusCode, Json<Value>) {
    let relative = value
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or(path.relative_path.as_str());
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = relative
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("");
    let content_type = mime_guess::from_path(relative)
        .first_or_text_plain()
        .essence_str()
        .to_string();
    (
        StatusCode::OK,
        Json(json!({
            "path": logical_path(path.project_id.as_str(), relative),
            "display_path": logical_path(path.project_id.as_str(), relative),
            "name": name,
            "size": value
                .get("size_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(content.len() as u64),
            "content_type": content_type,
            "is_binary": false,
            "writable": true,
            "modified_at": Value::Null,
            "content": content,
            "harness_project": true,
        })),
    )
}

fn search_entries_response(
    path: &HarnessProjectPath,
    value: Value,
    query: &str,
) -> (StatusCode, Json<Value>) {
    let entries = value
        .get("matches")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| normalize_entry(path.project_id.as_str(), entry, true))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (
        StatusCode::OK,
        Json(json!({
            "path": logical_path(path.project_id.as_str(), path.relative_path.as_str()),
            "query": query,
            "entries": entries,
            "truncated": value.get("truncated").and_then(Value::as_bool).unwrap_or(false),
            "visited_dirs": value.get("visited_dirs").and_then(Value::as_u64).unwrap_or(0),
            "harness_project": true,
        })),
    )
}

fn search_content_response(
    path: &HarnessProjectPath,
    value: Value,
    query: &str,
) -> (StatusCode, Json<Value>) {
    let entries = value
        .get("results")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let relative_path = item.get("path").and_then(Value::as_str)?;
                    let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                    let column = text
                        .find(query)
                        .map(|offset| text[..offset].chars().count() + 1)
                        .unwrap_or(1);
                    Some(json!({
                        "path": logical_path(path.project_id.as_str(), relative_path),
                        "relative_path": relative_path,
                        "line": item.get("line").and_then(Value::as_u64).unwrap_or(1),
                        "column": column,
                        "text": text,
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let returned_count = entries.len();
    (
        StatusCode::OK,
        Json(json!({
            "path": logical_path(path.project_id.as_str(), path.relative_path.as_str()),
            "query": query,
            "entries": entries,
            "truncated": value.get("count").and_then(Value::as_u64).unwrap_or(0) > returned_count as u64,
            "visited_dirs": value.get("scanned_files").and_then(Value::as_u64).unwrap_or(0),
            "harness_project": true,
        })),
    )
}

fn mutation_response(
    project_id: &str,
    value: Value,
    fallback_name: &str,
    created: bool,
) -> (StatusCode, Json<Value>) {
    let result = value.get("result").unwrap_or(&value);
    let relative = result
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = relative
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(fallback_name);
    (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(json!({
            "success": true,
            "path": logical_path(project_id, relative),
            "display_path": logical_path(project_id, relative),
            "name": name,
            "size": result.get("bytes").or_else(|| result.get("size")).cloned().unwrap_or(Value::Null),
            "created": created,
            "modified_at": Value::Null,
            "harness_project": true,
        })),
    )
}

fn created_response(
    project_id: &str,
    relative: &str,
    name: &str,
    created: bool,
) -> (StatusCode, Json<Value>) {
    (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(json!({
            "success": true,
            "path": logical_path(project_id, relative),
            "display_path": logical_path(project_id, relative),
            "name": name,
            "created": created,
            "harness_project": true,
        })),
    )
}

fn parse_harness_project_path(raw_path: &str) -> Option<HarnessProjectPath> {
    let url = Url::parse(raw_path.trim()).ok()?;
    if url.scheme() != HARNESS_PROJECT_SCHEME || url.host_str()? != HARNESS_PROJECT_HOST {
        return None;
    }
    let mut segments = url.path_segments()?;
    let project_id = segments.next()?.trim().to_string();
    if project_id.is_empty() {
        return None;
    }
    let relative_path = segments
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    Some(HarnessProjectPath {
        project_id,
        relative_path,
    })
}

fn harness_relative_arg(path: &HarnessProjectPath) -> String {
    if path.relative_path.is_empty() {
        ".".to_string()
    } else {
        path.relative_path.clone()
    }
}

fn child_relative_path(parent: &HarnessProjectPath, name: &str) -> String {
    if parent.relative_path.is_empty() {
        name.to_string()
    } else {
        format!("{}/{name}", parent.relative_path)
    }
}

fn logical_path(project_id: &str, relative_path: &str) -> String {
    let root = harness_project_root_path(project_id);
    let relative_path = relative_path.trim_matches('/');
    if relative_path.is_empty() || relative_path == "." {
        root
    } else {
        format!("{root}/{relative_path}")
    }
}

fn parent_logical_path(path: &HarnessProjectPath) -> Value {
    if path.relative_path.is_empty() {
        return Value::Null;
    }
    let parent = path
        .relative_path
        .rsplit_once('/')
        .map(|(value, _)| value)
        .unwrap_or("");
    Value::String(logical_path(path.project_id.as_str(), parent))
}

fn sort_entries(entries: &mut [Value]) {
    entries.sort_by(|left, right| {
        let left_dir = left.get("is_dir").and_then(Value::as_bool).unwrap_or(false);
        let right_dir = right
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if left_dir != right_dir {
            return right_dir.cmp(&left_dir);
        }
        let left_name = left
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        let right_name = right
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        left_name.cmp(&right_name)
    });
}

#[cfg(test)]
mod tests {
    use super::{logical_path, parse_harness_project_path, HarnessProjectPath};

    #[test]
    fn parses_harness_virtual_project_paths() {
        assert_eq!(
            parse_harness_project_path("harness://project/project-1/src/main.rs"),
            Some(HarnessProjectPath {
                project_id: "project-1".to_string(),
                relative_path: "src/main.rs".to_string(),
            })
        );
        assert!(parse_harness_project_path("/workspace/project-1").is_none());
    }

    #[test]
    fn builds_harness_logical_paths_without_exposing_git_url() {
        assert_eq!(
            logical_path("project-1", "src/main.rs"),
            "harness://project/project-1/src/main.rs"
        );
    }
}
