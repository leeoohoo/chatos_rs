// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::api::fs) fn is_harness_project_path(raw_path: &str) -> bool {
    parse_harness_project_path(raw_path).is_some()
}

pub(in crate::api::fs) async fn list_entries(
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

pub(in crate::api::fs) async fn read_file(
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

pub(in crate::api::fs) async fn search_entries(
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

pub(in crate::api::fs) async fn search_content(
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

pub(in crate::api::fs) async fn create_dir(
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

pub(in crate::api::fs) async fn create_file(
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

pub(in crate::api::fs) async fn write_file(
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

pub(in crate::api::fs) async fn delete_entry(
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

pub(in crate::api::fs) async fn download_entry(
    auth: &AuthUser,
    raw_path: &str,
) -> Option<Response> {
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
