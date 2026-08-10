// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn call_harness_tool(
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
        sync_secret,
        path.project_id.as_str(),
        tool_name,
        arguments,
    )
    .await
    .map_err(|err| (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))))
}

pub(super) async fn commit_harness_edit(
    auth: &AuthUser,
    path: &HarnessProjectPath,
    mut operation: Value,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let target_path = operation
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "path is required" })),
            )
        })?
        .to_string();
    if operation.get("expected_sha256").is_none() {
        let expected_sha256 = current_harness_revision(auth, path, target_path.as_str()).await?;
        operation["expected_sha256"] = expected_sha256.map(Value::String).unwrap_or(Value::Null);
    }

    let opened = call_harness_tool(auth, path, "open_edit_session", json!({})).await?;
    let session_id = opened
        .pointer("/result/session_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "Harness MCP did not return an edit session id" })),
            )
        })?
        .to_string();
    let staged = call_harness_tool(
        auth,
        path,
        "stage_edit_batch",
        json!({
            "session_id": session_id,
            "operations": [operation.clone()],
        }),
    )
    .await;
    if let Err(error) = staged {
        let _ = call_harness_tool(
            auth,
            path,
            "abort_edit_session",
            json!({ "session_id": session_id }),
        )
        .await;
        return Err(error);
    }
    let mut committed = call_harness_tool(
        auth,
        path,
        "commit_edit_session",
        json!({ "session_id": session_id }),
    )
    .await?;
    decorate_session_commit_result(&mut committed, target_path.as_str(), &operation);
    Ok(committed)
}

async fn current_harness_revision(
    auth: &AuthUser,
    path: &HarnessProjectPath,
    target_path: &str,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    match call_harness_tool(
        auth,
        path,
        "read_file_raw",
        json!({ "path": target_path, "with_line_numbers": false }),
    )
    .await
    {
        Ok(value) if value.get("status").and_then(Value::as_str) == Some("not_found") => Ok(None),
        Ok(value) => value
            .get("sha256")
            .and_then(Value::as_str)
            .map(|sha| Some(sha.to_string()))
            .ok_or_else(|| {
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": "Harness MCP read did not return sha256" })),
                )
            }),
        Err(error) if error_text(&error).contains("Target is not a file") => Ok(None),
        Err(error) => Err(error),
    }
}

fn decorate_session_commit_result(committed: &mut Value, path: &str, operation: &Value) {
    let result = committed
        .as_object_mut()
        .and_then(|payload| payload.get_mut("result"))
        .and_then(Value::as_object_mut);
    let Some(result) = result else {
        return;
    };
    result.insert("path".to_string(), Value::String(path.to_string()));
    if let Some(content) = operation.get("content").and_then(Value::as_str) {
        result.insert("bytes".to_string(), json!(content.len()));
    }
    if operation.get("kind").and_then(Value::as_str) == Some("delete") {
        result.insert("deleted".to_string(), Value::Bool(true));
    }
}

fn error_text(error: &(StatusCode, Json<Value>)) -> String {
    error.1 .0.to_string()
}

pub(super) async fn find_harness_entries(
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
