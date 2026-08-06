// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::Engine as _;
use serde_json::{json, Value};

use super::super::client::{
    fetch_harness_content, list_harness_paths, read_harness_file, read_harness_file_for_preview,
    HarnessContentInfo, HarnessDirContent, HarnessFile,
};
use super::super::path_policy::{
    optional_repo_path, path_matches_scope, path_name, required_file_path,
};
use super::super::{tool_structured_result, tool_text_result, HarnessMcpContext};

const DEFAULT_SEARCH_LIMIT: usize = 40;
const MAX_SEARCH_FILES: usize = 2_000;
const MAX_SEARCH_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const MISSING_FILE_CANDIDATE_LIMIT: usize = 20;
const MISSING_FILE_DIRECTORY_ENTRY_LIMIT: usize = 80;

pub(in super::super) async fn tool_read_file_raw(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let path = required_file_path(args)?;
    let with_line_numbers = args
        .get("with_line_numbers")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let encoding = args
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or("utf8");
    if encoding == "base64" {
        let file = match read_harness_file_for_preview(ctx, path.as_str()).await {
            Ok(file) => file,
            Err(err) if is_not_found_error(err.as_str()) => {
                return Ok(missing_file_discovery_result(
                    ctx,
                    path.as_str(),
                    "read_file_raw",
                    err.as_str(),
                )
                .await);
            }
            Err(err) => return Err(err),
        };
        return Ok(tool_structured_result(
            json!({
                "path": file.path,
                "size_bytes": file.size,
                "sha256": file.sha256,
                "content_encoding": "base64",
                "content": base64::engine::general_purpose::STANDARD.encode(file.bytes)
            }),
            "Binary file content returned as base64 in _structured_result.",
        ));
    }
    if encoding != "utf8" {
        return Err("encoding must be utf8 or base64".to_string());
    }
    let file = match read_harness_file(ctx, path.as_str()).await {
        Ok(file) => file,
        Err(err) if is_not_found_error(err.as_str()) => {
            return Ok(missing_file_discovery_result(
                ctx,
                path.as_str(),
                "read_file_raw",
                err.as_str(),
            )
            .await);
        }
        Err(err) => return Err(err),
    };
    Ok(tool_text_result(file_payload(&file, with_line_numbers)))
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
    let file = match read_harness_file(ctx, path.as_str()).await {
        Ok(file) => file,
        Err(err) if is_not_found_error(err.as_str()) => {
            return Ok(missing_file_discovery_result(
                ctx,
                path.as_str(),
                "read_file_range",
                err.as_str(),
            )
            .await);
        }
        Err(err) => return Err(err),
    };
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
        .map_err(|err| format!("parse project directory content failed: {err}"))?;
    let entries = dir
        .entries
        .into_iter()
        .take(max_entries)
        .map(directory_entry_payload)
        .collect::<Vec<_>>();
    Ok(tool_text_result(json!({ "entries": entries })))
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

async fn missing_file_discovery_result(
    ctx: &HarnessMcpContext,
    requested_path: &str,
    operation: &str,
    error: &str,
) -> Value {
    let path_candidates = match list_harness_paths(ctx).await {
        Ok(paths) => candidate_paths_for_missing_file(paths.files, requested_path),
        Err(err) => {
            return tool_text_result(json!({
                "status": "not_found",
                "original_read_failed": true,
                "operation": operation,
                "requested_path": requested_path,
                "content": Value::Null,
                "error": error,
                "message": "The requested file was not found. The automatic fallback path search also failed, so this result is not file content.",
                "ai_instruction": "Do not treat this as file content. Do not retry the same missing path. Use list_dir on the workspace root or a known directory before choosing another exact read path.",
                "fallback_discovery": {
                    "performed": true,
                    "path_search_failed": true,
                    "path_search_error": err,
                    "candidate_paths": Vec::<String>::new(),
                    "directory_listing": Value::Null
                }
            }));
        }
    };
    let directory_listing = missing_file_directory_listing(ctx, requested_path).await;
    tool_text_result(json!({
        "status": "not_found",
        "original_read_failed": true,
        "operation": operation,
        "requested_path": requested_path,
        "content": Value::Null,
        "error": error,
        "message": "The requested file was not found. This is fallback discovery, not file content.",
        "ai_instruction": "Do not treat this result as the requested file content. Do not retry the same missing path. If candidate_paths contains a plausible existing path, read that exact path once; otherwise inspect directory_listing or call list_dir/search_text before another read_file call.",
        "suggested_next_steps": [
            "Use one path from fallback_discovery.candidate_paths if it matches the intended file.",
            "If there are no candidates, inspect fallback_discovery.directory_listing or call list_dir on the relevant directory.",
            "For monorepos or nested projects, do not assume README/package/lock files live at the workspace root."
        ],
        "fallback_discovery": {
            "performed": true,
            "strategy": "project path search by requested name plus directory listing",
            "candidate_paths": path_candidates,
            "directory_listing": directory_listing
        }
    }))
}

async fn missing_file_directory_listing(ctx: &HarnessMcpContext, requested_path: &str) -> Value {
    let parent = parent_dir(requested_path);
    match directory_listing_payload(ctx, parent.as_str()).await {
        Ok(listing) => listing,
        Err(err) if !parent.is_empty() => match directory_listing_payload(ctx, "").await {
            Ok(mut root_listing) => {
                root_listing["requested_parent_path"] = json!(parent);
                root_listing["requested_parent_error"] = json!(err);
                root_listing
            }
            Err(root_err) => json!({
                "requested_parent_path": parent,
                "requested_parent_error": err,
                "root_error": root_err,
                "entries": Vec::<Value>::new()
            }),
        },
        Err(err) => json!({
            "path": parent,
            "error": err,
            "entries": Vec::<Value>::new()
        }),
    }
}

async fn directory_listing_payload(ctx: &HarnessMcpContext, path: &str) -> Result<Value, String> {
    let content = fetch_harness_content(ctx, path)
        .await
        .map_err(|err| err.to_string())?;
    if content.kind != "dir" {
        return Err("Target is not a directory.".to_string());
    }
    let dir: HarnessDirContent = serde_json::from_value(content.content)
        .map_err(|err| format!("parse project directory content failed: {err}"))?;
    let entries = dir
        .entries
        .into_iter()
        .take(MISSING_FILE_DIRECTORY_ENTRY_LIMIT)
        .map(directory_entry_payload)
        .collect::<Vec<_>>();
    Ok(json!({
        "path": path,
        "entries": entries
    }))
}

fn directory_entry_payload(entry: HarnessContentInfo) -> Value {
    json!({
        "name": if entry.name.is_empty() { path_name(entry.path.as_str()) } else { entry.name },
        "path": entry.path,
        "type": entry.kind,
        // Harness omits sizes from directory listings. Preserve that as null
        // instead of presenting an unknown file size as a real zero-byte file.
        "size": entry.size,
        "mtime_ms": 0
    })
}

fn candidate_paths_for_missing_file(paths: Vec<String>, requested_path: &str) -> Vec<String> {
    let requested = normalize_candidate_path(requested_path);
    let requested_name = path_name(requested.as_str()).to_ascii_lowercase();
    let requested_lower = requested.to_ascii_lowercase();
    let mut ranked = paths
        .into_iter()
        .filter_map(|path| {
            let candidate = normalize_candidate_path(path.as_str());
            let candidate_lower = candidate.to_ascii_lowercase();
            let score = if candidate_lower == requested_lower {
                0
            } else if !requested_lower.is_empty()
                && candidate_lower.ends_with(format!("/{requested_lower}").as_str())
            {
                1
            } else if path_name(candidate_lower.as_str()) == requested_name {
                2
            } else if !requested_name.is_empty()
                && candidate_lower.contains(requested_name.as_str())
            {
                3
            } else {
                return None;
            };
            Some((score, candidate))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    ranked.dedup_by(|left, right| left.1 == right.1);
    ranked
        .into_iter()
        .take(MISSING_FILE_CANDIDATE_LIMIT)
        .map(|(_, path)| path)
        .collect()
}

fn normalize_candidate_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .split('/')
        .filter(|part| !part.trim().is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn parent_dir(path: &str) -> String {
    let normalized = normalize_candidate_path(path);
    normalized
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default()
}

fn is_not_found_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("not found") || normalized.contains("wasn't found")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_entry_without_harness_size_stays_unknown() {
        let entry: HarnessContentInfo = serde_json::from_value(json!({
            "type": "file",
            "name": "README.md",
            "path": "README.md"
        }))
        .expect("directory entry");

        let payload = directory_entry_payload(entry);

        assert!(payload["size"].is_null());
    }

    #[test]
    fn file_payload_does_not_expose_storage_backend_details() {
        let payload = file_payload(
            &HarnessFile {
                path: "src/main.rs".to_string(),
                size: 11,
                sha256: "content-sha".to_string(),
                harness_blob_sha: "backend-blob-token".to_string(),
                content: "hello world".to_string(),
            },
            true,
        );
        let text = serde_json::to_string(&payload).expect("serialize payload");

        assert!(text.contains("src/main.rs"));
        assert!(!text.contains("harness"));
        assert!(!text.contains("backend-blob-token"));
    }

    #[test]
    fn missing_file_candidates_find_nested_files_without_claiming_content() {
        let candidates = candidate_paths_for_missing_file(
            vec![
                "apps/web/README.md".to_string(),
                "services/api/package-lock.json".to_string(),
                "docs/readme.md".to_string(),
                "src/main.rs".to_string(),
            ],
            "README.md",
        );

        assert_eq!(candidates, vec!["apps/web/README.md", "docs/readme.md"]);
    }

    #[test]
    fn missing_file_candidates_prefer_suffix_path_match() {
        let candidates = candidate_paths_for_missing_file(
            vec![
                "apps/admin/package.json".to_string(),
                "examples/apps/admin/package.json".to_string(),
                "apps/web/package.json".to_string(),
            ],
            "apps/admin/package.json",
        );

        assert_eq!(
            candidates,
            vec![
                "apps/admin/package.json",
                "examples/apps/admin/package.json",
                "apps/web/package.json"
            ]
        );
    }

    #[test]
    fn missing_file_parent_dir_uses_repo_root_for_root_files() {
        assert_eq!(parent_dir("README.md"), "");
        assert_eq!(parent_dir("./apps/web/package.json"), "apps/web");
    }

    #[test]
    fn not_found_detection_accepts_harness_messages() {
        assert!(is_not_found_error(
            "404 Not Found {\"message\":\"path 'README.md' wasn't found in the repo\"}"
        ));
        assert!(!is_not_found_error("File too large"));
    }
}
