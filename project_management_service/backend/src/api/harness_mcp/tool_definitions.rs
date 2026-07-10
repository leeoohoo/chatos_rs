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
            "description": "Return UTF-8 file content from the Harness repo for this cloud project. with_line_numbers defaults to true; set false to skip structured numbered lines.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "with_line_numbers": { "type": "boolean", "default": true }
                },
                "additionalProperties": false,
                "required": ["path"]
            }
        }),
        json!({
            "name": "read_file_range",
            "description": "Return UTF-8 content from start_line to end_line (1-based, inclusive) from the Harness repo for this cloud project.",
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
            "description": "List directory entries from the Harness repo for this cloud project.",
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
            "name": "list_branches",
            "description": "List branches from the internal Harness repo for this cloud project.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "search_text",
            "description": "Search text recursively under a directory in the Harness repo for this cloud project.",
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
            "description": "Write file content to the Harness repo for this cloud project. The write creates a Harness commit on the default branch.",
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
            "description": "Safely edit file content in the Harness repo by replacing old_text with new_text. Use before_context / after_context or start_line/end_line when old_text appears multiple times.",
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
            "description": "Append content to a file in the Harness repo for this cloud project. The write creates a Harness commit on the default branch.",
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
            "description": "Delete a file or a directory recursively from the Harness repo for this cloud project. Directory deletion creates one Harness commit with DELETE actions for tracked files under the directory.",
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
            "description": "Apply a patch to one or more files in the Harness repo for this cloud project. Supported formats match the builtin CodeMaintainer apply_patch tool. The write creates one Harness commit on the repository default branch.",
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
