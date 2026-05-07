use serde_json::{json, Value};

use super::service::{CodeMaintainerService, ToolContext};

pub(super) fn append_compat_aliases(service: &CodeMaintainerService, tools: &mut Vec<Value>) {
    if service.has_tool("read_file_raw") {
        tools.push(json!({
            "name": "read_file",
            "description": "Hermes-compatible alias. Read full file by default; when start_line + end_line are both provided, read a line range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 },
                    "with_line_numbers": { "type": "boolean", "default": true }
                },
                "additionalProperties": false,
                "required": ["path"]
            }
        }));
    }

    if service.has_tool("search_text") {
        tools.push(json!({
            "name": "search_files",
            "description": "Hermes-compatible alias of search_text. query maps to search_text.pattern.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "minLength": 1 },
                    "path": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "additionalProperties": false,
                "required": ["query"]
            }
        }));
    }

    if service.has_tool("apply_patch") {
        tools.push(json!({
            "name": "patch",
            "description": "Hermes-compatible alias of apply_patch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "patch": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false,
                "required": ["patch"]
            }
        }));
    }
}

pub(super) fn maybe_call_compat_tool(
    service: &CodeMaintainerService,
    name: &str,
    args: &Value,
    ctx: &ToolContext<'_>,
) -> Result<Option<Value>, String> {
    match name {
        "read_file" => call_read_file_alias(service, args, ctx).map(Some),
        "search_files" => call_search_files_alias(service, args, ctx).map(Some),
        "patch" => {
            if !service.has_tool("apply_patch") {
                return Err("Tool not found: patch".to_string());
            }
            service
                .call_registered_tool("apply_patch", args.clone(), ctx)
                .map(Some)
        }
        _ => Ok(None),
    }
}

fn call_read_file_alias(
    service: &CodeMaintainerService,
    args: &Value,
    ctx: &ToolContext<'_>,
) -> Result<Value, String> {
    let path = args
        .get("path")
        .and_then(|value| value.as_str())
        .ok_or("path is required".to_string())?;
    let start_line = args
        .get("start_line")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize);
    let end_line = args
        .get("end_line")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize);
    let with_line_numbers = args
        .get("with_line_numbers")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    match (start_line, end_line) {
        (Some(start_line), Some(end_line)) => {
            if !service.has_tool("read_file_range") {
                return Err("Tool not found: read_file_range".to_string());
            }
            let mapped_args = json!({
                "path": path,
                "start_line": start_line,
                "end_line": end_line,
                "with_line_numbers": with_line_numbers
            });
            service.call_registered_tool("read_file_range", mapped_args, ctx)
        }
        (None, None) => {
            if !service.has_tool("read_file_raw") {
                return Err("Tool not found: read_file_raw".to_string());
            }
            let mapped_args = json!({
                "path": path,
                "with_line_numbers": with_line_numbers
            });
            service.call_registered_tool("read_file_raw", mapped_args, ctx)
        }
        _ => Err(
            "start_line and end_line must be provided together for read_file".to_string(),
        ),
    }
}

fn call_search_files_alias(
    service: &CodeMaintainerService,
    args: &Value,
    ctx: &ToolContext<'_>,
) -> Result<Value, String> {
    let query = args
        .get("query")
        .or_else(|| args.get("pattern"))
        .and_then(|value| value.as_str())
        .ok_or("query is required".to_string())?;

    if !service.has_tool("search_text") {
        return Err("Tool not found: search_text".to_string());
    }

    let mut mapped = json!({
        "pattern": query
    });
    if let Some(path) = args.get("path").and_then(|value| value.as_str()) {
        mapped["path"] = json!(path);
    }
    if let Some(max_results) = args.get("max_results").and_then(|value| value.as_u64()) {
        mapped["max_results"] = json!(max_results);
    }

    service.call_registered_tool("search_text", mapped, ctx)
}
