// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::api::local_connectors::{
    call_local_mcp_tool, local_connector_root_path, parse_local_connector_root_path,
    LocalConnectorRootRef, LOCAL_CONNECTOR_BUILTIN_CODE_READ, LOCAL_CONNECTOR_BUILTIN_CODE_WRITE,
    LOCAL_CONNECTOR_BUILTIN_TERMINAL,
};

pub(super) async fn list_entries(
    raw_path: &str,
    include_files: bool,
) -> Option<(StatusCode, Json<Value>)> {
    let root_ref = parse_local_connector_root_path(raw_path)?;
    let relative_path = local_relative_arg(&root_ref);
    Some(
        match call_local_mcp_tool(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            None,
            &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
            "list_dir",
            json!({ "path": relative_path, "max_entries": 1000 }),
        )
        .await
        {
            Ok(value) => local_list_response(&root_ref, value, include_files),
            Err(err) => err,
        },
    )
}

pub(super) async fn search_entries(
    raw_path: &str,
    query: &str,
    limit: usize,
) -> Option<(StatusCode, Json<Value>)> {
    let root_ref = parse_local_connector_root_path(raw_path)?;
    Some(match find_local_entries(&root_ref, query, limit).await {
        Ok(value) => local_search_entries_response(&root_ref, value, query),
        Err(err) => err,
    })
}

pub(super) async fn read_file(raw_path: &str) -> Option<(StatusCode, Json<Value>)> {
    let root_ref = parse_local_connector_root_path(raw_path)?;
    let relative_path = local_relative_arg(&root_ref);
    Some(
        match call_local_mcp_tool(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            None,
            &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
            "read_file_raw",
            json!({ "path": relative_path, "with_line_numbers": false }),
        )
        .await
        {
            Ok(value) => local_read_response(&root_ref, value),
            Err(err) => err,
        },
    )
}

pub(super) async fn search_content(
    raw_path: &str,
    query: &str,
    limit: usize,
) -> Option<(StatusCode, Json<Value>)> {
    let root_ref = parse_local_connector_root_path(raw_path)?;
    let relative_path = local_relative_arg(&root_ref);
    Some(
        match call_local_mcp_tool(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            None,
            &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
            "search_text",
            json!({
                "path": relative_path,
                "pattern": query,
                "max_results": limit,
            }),
        )
        .await
        {
            Ok(value) => local_search_content_response(&root_ref, value, query),
            Err(err) => err,
        },
    )
}

pub(super) async fn create_dir(parent_path: &str, name: &str) -> Option<(StatusCode, Json<Value>)> {
    let target = local_child_relative_path(parent_path, name)?;
    let root_ref = parse_local_connector_root_path(parent_path)?;
    Some(
        match call_local_mcp_tool(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            None,
            &[LOCAL_CONNECTOR_BUILTIN_TERMINAL],
            "execute_command",
            json!({
                "path": ".",
                "common": format!("mkdir -- {}", shell_quote(target.as_str())),
                "background": false,
            }),
        )
        .await
        {
            Ok(value) if value.get("exit_code").and_then(Value::as_i64).unwrap_or(0) == 0 => {
                local_created_dir_response(&root_ref, target.as_str(), name)
            }
            Ok(value) => local_command_error_response(value),
            Err(err) => err,
        },
    )
}

pub(super) async fn create_file(
    parent_path: &str,
    name: &str,
    content: &str,
) -> Option<(StatusCode, Json<Value>)> {
    let target = local_child_relative_path(parent_path, name)?;
    let root_ref = parse_local_connector_root_path(parent_path)?;
    Some(
        match call_local_mcp_tool(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            None,
            &[LOCAL_CONNECTOR_BUILTIN_CODE_WRITE],
            "write_file",
            json!({
                "path": target,
                "content": content,
            }),
        )
        .await
        {
            Ok(value) => local_mutation_response(&root_ref, value, name, true),
            Err(err) => err,
        },
    )
}

pub(super) async fn write_file(raw_path: &str, content: &str) -> Option<(StatusCode, Json<Value>)> {
    let root_ref = parse_local_connector_root_path(raw_path)?;
    let relative_path = local_relative_arg(&root_ref);
    let name = relative_path
        .rsplit('/')
        .find(|part| !part.trim().is_empty())
        .unwrap_or("");
    Some(
        match call_local_mcp_tool(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            None,
            &[LOCAL_CONNECTOR_BUILTIN_CODE_WRITE],
            "write_file",
            json!({
                "path": relative_path,
                "content": content,
            }),
        )
        .await
        {
            Ok(value) => local_mutation_response(&root_ref, value, name, false),
            Err(err) => err,
        },
    )
}

fn local_list_response(
    root_ref: &LocalConnectorRootRef,
    value: Value,
    include_files: bool,
) -> (StatusCode, Json<Value>) {
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .map(|path| logical_path(root_ref, path))
        .unwrap_or_else(|| logical_path(root_ref, local_relative_arg(root_ref).as_str()));
    let mut entries = value
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| normalize_entry(root_ref, entry, include_files))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
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

    (
        StatusCode::OK,
        Json(json!({
            "path": path,
            "display_path": path,
            "parent": Value::Null,
            "writable": true,
            "entries": entries,
            "roots": Vec::<Value>::new(),
            "local_connector": true,
        })),
    )
}

fn normalize_entry(
    root_ref: &LocalConnectorRootRef,
    entry: &Value,
    include_files: bool,
) -> Option<Value> {
    let is_dir = entry
        .get("is_dir")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| entry.get("type").and_then(Value::as_str) == Some("dir"));
    if !is_dir && !include_files {
        return None;
    }
    let name = entry
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let raw_path = entry.get("path").and_then(Value::as_str)?;
    let path = logical_path(root_ref, raw_path);
    Some(json!({
        "name": name,
        "path": path,
        "display_path": path,
        "is_dir": is_dir,
        "size": entry.get("len").or_else(|| entry.get("size")).cloned().unwrap_or(Value::Null),
        "modified_at": entry
            .get("modified_at")
            .or_else(|| entry.get("mtime_ms"))
            .cloned()
            .unwrap_or(Value::Null),
    }))
}

fn local_read_response(
    root_ref: &LocalConnectorRootRef,
    value: Value,
) -> (StatusCode, Json<Value>) {
    let relative = value
        .get("path")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| local_relative_arg(root_ref));
    let path = logical_path(root_ref, relative.as_str());
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let size = value
        .get("len")
        .or_else(|| value.get("size"))
        .and_then(Value::as_u64)
        .unwrap_or(content.len() as u64);
    let name = relative
        .rsplit('/')
        .find(|part| !part.trim().is_empty())
        .unwrap_or("");
    let content_type = mime_guess::from_path(relative.as_str())
        .first_or_text_plain()
        .essence_str()
        .to_string();
    (
        StatusCode::OK,
        Json(json!({
            "path": path,
            "display_path": path,
            "name": name,
            "size": size,
            "content_type": content_type,
            "is_binary": false,
            "writable": true,
            "modified_at": Value::Null,
            "content": content,
            "local_connector": true,
        })),
    )
}

fn local_search_entries_response(
    root_ref: &LocalConnectorRootRef,
    value: Value,
    query: &str,
) -> (StatusCode, Json<Value>) {
    let entries = value
        .get("matches")
        .or_else(|| value.get("results"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| normalize_found_entry(root_ref, item))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let path = logical_path(root_ref, local_relative_arg(root_ref).as_str());
    (
        StatusCode::OK,
        Json(json!({
            "path": path,
            "query": query,
            "entries": entries,
            "truncated": value.get("truncated").and_then(Value::as_bool).unwrap_or(false),
            "visited_dirs": value.get("visited_dirs").and_then(Value::as_u64).unwrap_or(0),
            "local_connector": true,
        })),
    )
}

fn normalize_found_entry(root_ref: &LocalConnectorRootRef, item: &Value) -> Option<Value> {
    let raw_path = item.get("path").and_then(Value::as_str)?;
    let path = logical_path(root_ref, raw_path);
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| raw_path.rsplit('/').find(|part| !part.trim().is_empty()))
        .unwrap_or("");
    Some(json!({
        "name": name,
        "path": path,
        "display_path": path,
        "relative_path": project_relative_path(root_ref, raw_path),
        "is_dir": item
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| item.get("type").and_then(Value::as_str) == Some("dir")),
        "size": item.get("len").or_else(|| item.get("size")).cloned().unwrap_or(Value::Null),
        "modified_at": item
            .get("modified_at")
            .or_else(|| item.get("mtime_ms"))
            .cloned()
            .unwrap_or(Value::Null),
    }))
}

fn local_search_content_response(
    root_ref: &LocalConnectorRootRef,
    value: Value,
    query: &str,
) -> (StatusCode, Json<Value>) {
    let entries = value
        .get("matches")
        .or_else(|| value.get("results"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| normalize_search_match(root_ref, item, query))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let path = logical_path(root_ref, local_relative_arg(root_ref).as_str());
    (
        StatusCode::OK,
        Json(json!({
            "path": path,
            "query": query,
            "entries": entries,
            "truncated": value.get("truncated").and_then(Value::as_bool).unwrap_or(false),
            "visited_dirs": value
                .get("visited_dirs")
                .or_else(|| value.get("scanned_files"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "local_connector": true,
        })),
    )
}

fn normalize_search_match(
    root_ref: &LocalConnectorRootRef,
    item: &Value,
    query: &str,
) -> Option<Value> {
    let raw_path = item.get("path").and_then(Value::as_str)?;
    let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
    let column = item
        .get("column")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .or_else(|| {
            text.find(query)
                .map(|offset| text[..offset].chars().count() + 1)
        })
        .unwrap_or(1);
    Some(json!({
        "path": logical_path(root_ref, raw_path),
        "relative_path": project_relative_path(root_ref, raw_path),
        "line": item.get("line").and_then(Value::as_u64).unwrap_or(1),
        "column": column,
        "text": text,
    }))
}

fn local_mutation_response(
    root_ref: &LocalConnectorRootRef,
    value: Value,
    fallback_name: &str,
    created: bool,
) -> (StatusCode, Json<Value>) {
    let result = value.get("result").unwrap_or(&value);
    let relative = result
        .get("path")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| local_relative_arg(root_ref));
    let path = logical_path(root_ref, relative.as_str());
    let parent = relative
        .rsplit_once('/')
        .map(|(parent, _)| logical_path(root_ref, parent))
        .unwrap_or_else(|| {
            local_connector_root_path(
                root_ref.device_id.as_str(),
                root_ref.workspace_id.as_str(),
                None,
            )
        });
    let name = relative
        .rsplit('/')
        .find(|part| !part.trim().is_empty())
        .unwrap_or(fallback_name);
    (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(json!({
            "success": true,
            "path": path,
            "display_path": path,
            "parent": parent,
            "name": name,
            "size": result.get("bytes").or_else(|| result.get("size")).cloned().unwrap_or(Value::Null),
            "created": created,
            "modified_at": Value::Null,
            "local_connector": true,
        })),
    )
}

fn local_created_dir_response(
    root_ref: &LocalConnectorRootRef,
    relative: &str,
    fallback_name: &str,
) -> (StatusCode, Json<Value>) {
    let path = logical_path(root_ref, relative);
    let parent = relative
        .rsplit_once('/')
        .map(|(parent, _)| logical_path(root_ref, parent))
        .unwrap_or_else(|| {
            local_connector_root_path(
                root_ref.device_id.as_str(),
                root_ref.workspace_id.as_str(),
                None,
            )
        });
    let name = relative
        .rsplit('/')
        .find(|part| !part.trim().is_empty())
        .unwrap_or(fallback_name);
    (
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "path": path,
            "display_path": path,
            "parent": parent,
            "name": name,
            "created": true,
            "local_connector": true,
        })),
    )
}

fn local_command_error_response(value: Value) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({
            "error": "Local Connector 命令执行失败",
            "detail": value,
        })),
    )
}

async fn find_local_entries(
    root_ref: &LocalConnectorRootRef,
    query: &str,
    limit: usize,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let max_results = limit.max(1);
    let query_lower = query.trim().to_lowercase();
    let mut stack = vec![local_relative_arg(root_ref)];
    let mut matches = Vec::new();
    let mut visited_dirs = 0usize;

    while let Some(path) = stack.pop() {
        if matches.len() >= max_results {
            break;
        }
        let value = call_local_mcp_tool(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            None,
            &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
            "list_dir",
            json!({ "path": path, "max_entries": 1000 }),
        )
        .await?;
        visited_dirs += 1;
        let entries = value
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for entry in entries {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let raw_path = entry
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let path_lower = raw_path.to_lowercase();
            let name_lower = name.to_lowercase();
            let is_dir = entry
                .get("is_dir")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| entry.get("type").and_then(Value::as_str) == Some("dir"));
            if name_lower.contains(query_lower.as_str())
                || path_lower.contains(query_lower.as_str())
            {
                matches.push(entry.clone());
                if matches.len() >= max_results {
                    break;
                }
            }
            if is_dir {
                stack.push(raw_path.to_string());
            }
        }
    }

    matches.sort_by(|left, right| {
        let left_path = left
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        let right_path = right
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        left_path.cmp(&right_path)
    });
    Ok(json!({
        "query": query,
        "matches": matches,
        "visited_dirs": visited_dirs,
        "truncated": matches.len() >= max_results,
    }))
}

fn local_child_relative_path(parent_path: &str, name: &str) -> Option<String> {
    let root_ref = parse_local_connector_root_path(parent_path)?;
    let base = root_ref.relative_path.unwrap_or_default();
    if base.trim().is_empty() {
        Some(name.to_string())
    } else {
        Some(format!("{base}/{name}"))
    }
}

fn local_relative_arg(root_ref: &LocalConnectorRootRef) -> String {
    root_ref
        .relative_path
        .clone()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| ".".to_string())
}

fn logical_path(root_ref: &LocalConnectorRootRef, relative_path: &str) -> String {
    let relative_path = relative_path.trim();
    local_connector_root_path(
        root_ref.device_id.as_str(),
        root_ref.workspace_id.as_str(),
        if relative_path.is_empty() || relative_path == "." {
            None
        } else {
            Some(relative_path)
        },
    )
}

fn project_relative_path(root_ref: &LocalConnectorRootRef, relative_path: &str) -> String {
    let root_relative = root_ref.relative_path.as_deref().unwrap_or("");
    if root_relative.is_empty() {
        return relative_path.to_string();
    }
    relative_path
        .strip_prefix(root_relative)
        .and_then(|value| value.strip_prefix('/').or(Some(value)))
        .unwrap_or(relative_path)
        .to_string()
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}
