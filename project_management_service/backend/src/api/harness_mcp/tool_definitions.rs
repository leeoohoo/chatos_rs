// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_service::HostCapabilityPolicy;
use serde_json::{json, Value};

pub(super) fn tool_definitions(enabled: &HostCapabilityPolicy) -> Vec<Value> {
    let mut tools = Vec::new();
    if enabled.code_read {
        tools.extend(read_tool_definitions());
    }
    if enabled.code_write {
        tools.extend(write_tool_definitions());
    }
    tools
}

fn read_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "read_file_raw",
            "description": "Return file content from the current project workspace. encoding defaults to utf8; base64 is intended for binary preview consumers. with_line_numbers defaults to true for UTF-8 reads. If the requested file is missing, returns status=not_found with fallback_discovery candidate paths and directory entries; that fallback is not file content.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "with_line_numbers": { "type": "boolean", "default": true },
                    "encoding": { "type": "string", "enum": ["utf8", "base64"], "default": "utf8" }
                },
                "additionalProperties": false,
                "required": ["path"]
            }
        }),
        json!({
            "name": "read_file_range",
            "description": "Return UTF-8 content from start_line to end_line (1-based, inclusive) from the current project workspace. If the requested file is missing, returns status=not_found with fallback_discovery candidate paths and directory entries; that fallback is not file content.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 },
                    "with_line_numbers": { "type": "boolean" }
                },
                "additionalProperties": false,
                "required": ["path", "start_line", "end_line"]
            }
        }),
        json!({
            "name": "list_dir",
            "description": "List directory entries from the current project workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_entries": { "type": "integer", "minimum": 1, "maximum": 1000 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "search_text",
            "description": "Search text recursively under a directory in the current project workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "minLength": 1 },
                    "path": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "additionalProperties": false,
                "required": ["pattern"]
            }
        }),
    ]
}

fn write_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "write_file",
            "description": "Write file content to the current project workspace. Use this for new files or full-file replacement when the target path is known.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "additionalProperties": false,
                "required": ["path", "content"]
            }
        }),
        json!({
            "name": "edit_file",
            "description": "Safely edit a file in the current project workspace by replacing old_text with new_text. Use before_context / after_context or start_line/end_line when old_text appears multiple times. Context may be supplied as adjacent whole lines without manually adding the boundary newline.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_text": { "type": "string", "minLength": 1 },
                    "new_text": { "type": "string" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 },
                    "before_context": { "type": "string" },
                    "after_context": { "type": "string" },
                    "expected_matches": { "type": "integer", "minimum": 1 }
                },
                "additionalProperties": false,
                "required": ["path", "old_text", "new_text"]
            }
        }),
        json!({
            "name": "append_file",
            "description": "Append content to a file in the current project workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "additionalProperties": false,
                "required": ["path", "content"]
            }
        }),
        json!({
            "name": "delete_path",
            "description": "Delete a file or directory recursively from the current project workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "additionalProperties": false,
                "required": ["path"]
            }
        }),
        json!({
            "name": "apply_patch",
            "description": "Apply a patch to one or more files in the current project workspace. Supported formats match the builtin CodeMaintainer apply_patch tool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "patch": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false,
                "required": ["patch"]
            }
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_descriptions_expose_project_workspace_semantics_only() {
        let tools = tool_definitions(&HostCapabilityPolicy::from_builtin_kind_names([
            "CodeMaintainerRead",
            "CodeMaintainerWrite",
        ]));
        let text = serde_json::to_string(&tools).expect("serialize tool definitions");
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert!(text.contains("current project workspace"));
        assert!(!names.contains(&"list_branches"));
        assert!(!text.contains("Harness repo"));
        assert!(!text.contains("internal Harness"));
        assert!(!text.contains("default branch"));
        assert!(!text.contains("creates a Harness commit"));
    }
}
