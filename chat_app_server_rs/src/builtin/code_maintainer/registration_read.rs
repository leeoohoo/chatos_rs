use serde_json::{json, Value};
use std::sync::Arc;

use super::fs_ops::FsOps;
use super::service::CodeMaintainerService;
use super::utils::format_bytes;

use crate::core::tool_io::text_result;

pub(super) fn register_read_tools(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    workspace_note: &str,
    max_file_bytes: i64,
) {
    register_read_file_raw_tool(service, fs_ops.clone(), workspace_note);
    register_read_file_range_tool(service, fs_ops.clone(), workspace_note, max_file_bytes);
    register_list_dir_tool(service, fs_ops.clone(), workspace_note);
    register_search_text_tool(service, fs_ops, workspace_note);
}

fn register_read_file_raw_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    workspace_note: &str,
) {
    service.register_tool(
        "read_file_raw",
        &format!(
            "Return UTF-8 file content.\nwith_line_numbers defaults to true; set false to skip structured numbered lines.\n{workspace_note}"
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "with_line_numbers": { "type": "boolean", "default": true }
            },
            "additionalProperties": false,
            "required": ["path"]
        }),
        Arc::new(move |args, _ctx| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("path is required".to_string())?;
            let with_line_numbers = args
                .get("with_line_numbers")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            let (path, size, sha256, content) = fs_ops.read_file_raw(path)?;
            let normalized_lines: Vec<String> = content
                .split('\n')
                .map(|line| line.trim_end_matches('\r').to_string())
                .collect();
            let line_count = normalized_lines.len();
            let ends_with_newline = content.ends_with('\n');
            let numbered_lines = if with_line_numbers {
                Some(
                    normalized_lines
                        .iter()
                        .enumerate()
                        .map(|(idx, text)| {
                            json!({
                                "line": idx + 1,
                                "text": text
                            })
                        })
                        .collect::<Vec<Value>>(),
                )
            } else {
                None
            };
            let mut payload = json!({
                "path": path,
                "size_bytes": size,
                "sha256": sha256,
                "line_count": line_count,
                "ends_with_newline": ends_with_newline,
                "content": content
            });
            if let Some(lines) = numbered_lines {
                payload["numbered_lines"] = Value::Array(lines);
            }
            Ok(text_result(payload))
        }),
    );
}

fn register_read_file_range_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    workspace_note: &str,
    max_file_bytes: i64,
) {
    service.register_tool(
        "read_file_range",
        &format!(
            "Return UTF-8 content from start_line to end_line (1-based, inclusive).\nFile size limit: {}.\n{workspace_note}",
            format_bytes(max_file_bytes)
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "start_line": { "type": "integer", "minimum": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
                "with_line_numbers": { "type": "boolean" }
            },
            "additionalProperties": false,
            "required": ["path", "start_line", "end_line"]
        }),
        Arc::new(move |args, _ctx| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("path is required".to_string())?;
            let start_line = args
                .get("start_line")
                .and_then(|value| value.as_u64())
                .ok_or("start_line is required".to_string())? as usize;
            let end_line = args
                .get("end_line")
                .and_then(|value| value.as_u64())
                .ok_or("end_line is required".to_string())? as usize;
            let with_numbers = args
                .get("with_line_numbers")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let (path, size, sha256, start, end, total, content) =
                fs_ops.read_file_range(path, start_line, end_line, with_numbers)?;
            Ok(text_result(json!({
                "path": path,
                "size_bytes": size,
                "sha256": sha256,
                "start_line": start,
                "end_line": end,
                "total_lines": total,
                "content": content
            })))
        }),
    );
}

fn register_list_dir_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    workspace_note: &str,
) {
    service.register_tool(
        "list_dir",
        &format!("List directory entries.\n{workspace_note}"),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "max_entries": { "type": "integer", "minimum": 1, "maximum": 1000 }
            },
            "additionalProperties": false
        }),
        Arc::new(move |args, _ctx| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            let max_entries = args
                .get("max_entries")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize)
                .unwrap_or(200);
            let entries = fs_ops.list_dir(path, max_entries)?;
            Ok(text_result(json!({ "entries": entries })))
        }),
    );
}

fn register_search_text_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    workspace_note: &str,
) {
    service.register_tool(
        "search_text",
        &format!("Search text recursively under a directory.\n{workspace_note}"),
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "minLength": 1 },
                "path": { "type": "string" },
                "max_results": { "type": "integer", "minimum": 1, "maximum": 500 }
            },
            "additionalProperties": false,
            "required": ["pattern"]
        }),
        Arc::new(move |args, _ctx| {
            let pattern = args
                .get("pattern")
                .and_then(|value| value.as_str())
                .ok_or("pattern is required".to_string())?;
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            let max_results = args
                .get("max_results")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize);
            let results = fs_ops.search_text(pattern, path, max_results)?;
            Ok(text_result(
                json!({ "count": results.len(), "results": results }),
            ))
        }),
    );
}
