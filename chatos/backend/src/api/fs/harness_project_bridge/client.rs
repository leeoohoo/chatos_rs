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
        cfg.project_service_base_url.as_str(),
        sync_secret,
        path.project_id.as_str(),
        tool_name,
        arguments,
    )
    .await
    .map_err(|err| (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))))
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
