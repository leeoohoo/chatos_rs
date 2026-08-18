// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::api::local_connectors::{
    call_local_workspace_filesystem, local_connector_root_path, parse_local_connector_root_path,
    LocalConnectorRootRef,
};

pub(super) async fn list_entries(
    raw_path: &str,
    include_files: bool,
) -> Option<(StatusCode, Json<Value>)> {
    let root_ref = parse_local_connector_root_path(raw_path)?;
    let relative_path = local_relative_arg(&root_ref);
    Some(
        match call_local_workspace_filesystem(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            json!({ "operation": "list", "path": relative_path }),
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
    Some(
        match call_local_workspace_filesystem(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            json!({
                "operation": "search_entries",
                "path": local_relative_arg(&root_ref),
                "query": query,
                "limit": limit,
            }),
        )
        .await
        {
            Ok(value) => local_search_entries_response(&root_ref, value, query),
            Err(err) => err,
        },
    )
}

pub(super) async fn read_file(raw_path: &str) -> Option<(StatusCode, Json<Value>)> {
    let root_ref = parse_local_connector_root_path(raw_path)?;
    let relative_path = local_relative_arg(&root_ref);
    Some(
        match call_local_workspace_filesystem(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            json!({ "operation": "read", "path": relative_path }),
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
        match call_local_workspace_filesystem(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            json!({
                "operation": "search_content",
                "path": relative_path,
                "query": query,
                "limit": limit,
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
        match call_local_workspace_filesystem(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            json!({ "operation": "create_directory", "path": target }),
        )
        .await
        {
            Ok(_) => local_created_dir_response(&root_ref, target.as_str(), name),
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
        match call_local_workspace_filesystem(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            json!({
                "operation": "create_file",
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
        match call_local_workspace_filesystem(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            json!({
                "operation": "write_file",
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

pub(super) async fn delete_entry(
    raw_path: &str,
    recursive: bool,
) -> Option<(StatusCode, Json<Value>)> {
    let root_ref = parse_local_connector_root_path(raw_path)?;
    let relative_path = local_relative_arg(&root_ref);
    Some(
        match call_local_workspace_filesystem(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            json!({
                "operation": "delete",
                "path": relative_path,
                "recursive": recursive,
            }),
        )
        .await
        {
            Ok(value) => local_delete_response(&root_ref, value),
            Err(err) => err,
        },
    )
}

pub(super) async fn move_entry(
    source_path: &str,
    target_parent_path: &str,
    requested_target_name: Option<&str>,
    replace_existing: bool,
) -> Option<(StatusCode, Json<Value>)> {
    let source_ref = parse_local_connector_root_path(source_path);
    let target_ref = parse_local_connector_root_path(target_parent_path);
    if source_ref.is_none() && target_ref.is_none() {
        return None;
    }
    let (Some(source_ref), Some(target_ref)) = (source_ref, target_ref) else {
        return Some((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Local Connector 条目只能在同一工作区内移动" })),
        ));
    };
    if source_ref.device_id != target_ref.device_id
        || source_ref.workspace_id != target_ref.workspace_id
    {
        return Some((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Local Connector 条目只能在同一工作区内移动" })),
        ));
    }
    let source_relative = local_relative_arg(&source_ref);
    let source_name = source_relative
        .rsplit('/')
        .find(|part| !part.trim().is_empty())
        .unwrap_or_default();
    let target_name = requested_target_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(source_name);
    if target_name.is_empty()
        || target_name == "."
        || target_name == ".."
        || target_name.contains('/')
        || target_name.contains('\\')
    {
        return Some((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目标名称不合法" })),
        ));
    }
    let target_relative = local_child_relative_path(target_parent_path, target_name)?;
    Some(
        match call_local_workspace_filesystem(
            source_ref.device_id.as_str(),
            source_ref.workspace_id.as_str(),
            json!({
                "operation": "move",
                "source_path": source_relative,
                "target_path": target_relative,
                "replace_existing": replace_existing,
            }),
        )
        .await
        {
            Ok(value) => local_move_response(&source_ref, value),
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

    let parent = value
        .get("parent")
        .and_then(Value::as_str)
        .map(|parent| logical_path(root_ref, parent));
    (
        StatusCode::OK,
        Json(json!({
            "path": path,
            "display_path": path,
            "parent": parent,
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
    let is_binary = value
        .get("is_binary")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (
        StatusCode::OK,
        Json(json!({
            "path": path,
            "display_path": path,
            "name": name,
            "size": size,
            "content_type": content_type,
            "is_binary": is_binary,
            "writable": true,
            "modified_at": value.get("modified_at").cloned().unwrap_or(Value::Null),
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

fn local_delete_response(
    root_ref: &LocalConnectorRootRef,
    value: Value,
) -> (StatusCode, Json<Value>) {
    let relative = value
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_else(|| root_ref.relative_path.as_deref().unwrap_or("."));
    let path = logical_path(root_ref, relative);
    (
        StatusCode::OK,
        Json(json!({
            "path": path,
            "display_path": path,
            "is_dir": value.get("is_dir").and_then(Value::as_bool).unwrap_or(false),
            "recursive": value.get("recursive").and_then(Value::as_bool).unwrap_or(false),
            "deleted": value.get("deleted").and_then(Value::as_bool).unwrap_or(true),
            "local_connector": true,
        })),
    )
}

fn local_move_response(
    root_ref: &LocalConnectorRootRef,
    value: Value,
) -> (StatusCode, Json<Value>) {
    let from_relative = value
        .get("from_path")
        .and_then(Value::as_str)
        .unwrap_or_else(|| root_ref.relative_path.as_deref().unwrap_or("."));
    let to_relative = value
        .get("to_path")
        .and_then(Value::as_str)
        .unwrap_or(from_relative);
    let from_path = logical_path(root_ref, from_relative);
    let to_path = logical_path(root_ref, to_relative);
    (
        StatusCode::OK,
        Json(json!({
            "from_path": from_path,
            "to_path": to_path,
            "display_path": to_path,
            "name": value.get("name").cloned().unwrap_or(Value::Null),
            "replaced": value.get("replaced").and_then(Value::as_bool).unwrap_or(false),
            "is_dir": value.get("is_dir").and_then(Value::as_bool).unwrap_or(false),
            "moved": value.get("moved").and_then(Value::as_bool).unwrap_or(true),
            "local_connector": true,
        })),
    )
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
