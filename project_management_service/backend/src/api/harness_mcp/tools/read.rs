// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::super::client::{
    fetch_harness_content, list_harness_branches, list_harness_paths, read_harness_file,
    HarnessDirContent, HarnessFile,
};
use super::super::path_policy::{
    optional_repo_path, path_matches_scope, path_name, required_file_path,
};
use super::super::{tool_text_result, HarnessMcpContext};

const DEFAULT_SEARCH_LIMIT: usize = 40;
const MAX_SEARCH_FILES: usize = 2_000;
const MAX_SEARCH_TOTAL_BYTES: usize = 8 * 1024 * 1024;

pub(in super::super) async fn tool_read_file_raw(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let with_line_numbers = args
        .get("with_line_numbers")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let file = read_harness_file(ctx, path.as_str()).await?;
    let mut payload = file_payload(&file, with_line_numbers);
    payload["harness"] = json!({
        "project_id": ctx.project_id,
        "repo_path": ctx.repo_path,
        "blob_sha": file.harness_blob_sha
    });
    Ok(tool_text_result(payload))
}

pub(in super::super) async fn tool_read_file_range(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let start_line = args
        .get("start_line")
        .and_then(Value::as_u64)
        .ok_or_else(|| "start_line is required".to_string())? as usize;
    let end_line = args
        .get("end_line")
        .and_then(Value::as_u64)
        .ok_or_else(|| "end_line is required".to_string())? as usize;
    let with_numbers = args
        .get("with_line_numbers")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let file = read_harness_file(ctx, path.as_str()).await?;
    let lines = normalized_lines(file.content.as_str());
    let total_lines = lines.len();
    let start = start_line.max(1);
    let end = end_line.min(total_lines.max(1));
    let selected = if start <= end_line {
        lines
            .iter()
            .enumerate()
            .filter_map(|(idx, line)| {
                let line_no = idx + 1;
                (line_no >= start && line_no <= end_line).then(|| {
                    if with_numbers {
                        format!("{line_no}: {line}")
                    } else {
                        line.clone()
                    }
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    Ok(tool_text_result(json!({
        "path": file.path,
        "size_bytes": file.size,
        "sha256": file.sha256,
        "harness_blob_sha": file.harness_blob_sha,
        "start_line": start,
        "end_line": end,
        "total_lines": total_lines,
        "content": selected.join("\n")
    })))
}

pub(in super::super) async fn tool_list_dir(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = optional_repo_path(args.get("path").and_then(Value::as_str), true)?;
    let max_entries = args
        .get("max_entries")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 1000) as usize)
        .unwrap_or(200);
    let content = fetch_harness_content(ctx, path.as_str())
        .await
        .map_err(|err| err.to_string())?;
    if content.kind != "dir" {
        return Err("Target is not a directory.".to_string());
    }
    let dir: HarnessDirContent = serde_json::from_value(content.content)
        .map_err(|err| format!("parse Harness directory content failed: {err}"))?;
    let entries = dir
        .entries
        .into_iter()
        .take(max_entries)
        .map(|entry| {
            json!({
                "name": if entry.name.is_empty() { path_name(entry.path.as_str()) } else { entry.name },
                "path": entry.path,
                "type": entry.kind,
                "size": 0,
                "mtime_ms": 0
            })
        })
        .collect::<Vec<_>>();
    Ok(tool_text_result(json!({ "entries": entries })))
}

pub(in super::super) async fn tool_list_branches(
    ctx: &HarnessMcpContext,
    _args: &Value,
) -> Result<Value, String> {
    let branches = list_harness_branches(ctx).await?;
    let current = branches
        .iter()
        .find(|branch| branch.is_default)
        .or_else(|| branches.first())
        .map(|branch| branch.name.clone());
    Ok(tool_text_result(json!({
        "current": current,
        "branches": branches.into_iter().map(|branch| json!({
            "name": branch.name,
            "sha": branch.sha,
            "is_default": branch.is_default,
        })).collect::<Vec<_>>()
    })))
}

pub(in super::super) async fn tool_search_text(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let pattern = args
        .get("pattern")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "pattern is required".to_string())?;
    let scope = optional_repo_path(args.get("path").and_then(Value::as_str), true)?;
    let limit = args
        .get("max_results")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 500) as usize)
        .unwrap_or(DEFAULT_SEARCH_LIMIT);
    let paths = list_harness_paths(ctx).await?;
    let mut results = Vec::new();
    let mut visited_files = 0usize;
    let mut visited_bytes = 0usize;
    for file_path in paths
        .files
        .into_iter()
        .filter(|path| path_matches_scope(path, scope.as_str()))
    {
        if results.len() >= limit {
            break;
        }
        visited_files += 1;
        if visited_files > MAX_SEARCH_FILES {
            break;
        }
        let file = match read_harness_file(ctx, file_path.as_str()).await {
            Ok(file) => file,
            Err(_) => continue,
        };
        visited_bytes = visited_bytes.saturating_add(file.content.len());
        if visited_bytes > MAX_SEARCH_TOTAL_BYTES {
            break;
        }
        for (idx, line) in normalized_lines(file.content.as_str())
            .into_iter()
            .enumerate()
        {
            if line.contains(pattern) {
                results.push(json!({
                    "path": file.path,
                    "line": idx + 1,
                    "text": truncate_search_text(line.as_str())
                }));
                if results.len() >= limit {
                    break;
                }
            }
        }
    }
    Ok(tool_text_result(json!({
        "count": results.len(),
        "results": results,
        "scanned_files": visited_files,
        "scanned_bytes": visited_bytes
    })))
}

fn normalized_lines(content: &str) -> Vec<String> {
    content
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}

fn file_payload(file: &HarnessFile, with_line_numbers: bool) -> Value {
    let lines = normalized_lines(file.content.as_str());
    let mut payload = json!({
        "path": file.path,
        "size_bytes": file.size,
        "sha256": file.sha256,
        "harness_blob_sha": file.harness_blob_sha,
        "line_count": lines.len(),
        "ends_with_newline": file.content.ends_with('\n'),
        "content": file.content
    });
    if with_line_numbers {
        payload["numbered_lines"] = Value::Array(
            lines
                .iter()
                .enumerate()
                .map(|(idx, text)| {
                    json!({
                        "line": idx + 1,
                        "text": text
                    })
                })
                .collect(),
        );
    }
    payload
}

fn truncate_search_text(value: &str) -> String {
    const LIMIT: usize = 500;
    if value.chars().count() <= LIMIT {
        return value.to_string();
    }
    let mut text = value.chars().take(LIMIT).collect::<String>();
    text.push_str("...");
    text
}
