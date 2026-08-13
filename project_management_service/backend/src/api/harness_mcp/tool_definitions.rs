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
    if enabled.terminal {
        tools.extend(terminal_tool_definitions());
    }
    tools
}

fn terminal_tool_definitions() -> Vec<Value> {
    chatos_mcp::builtin_tool_catalog(chatos_mcp_runtime::BuiltinMcpKind::TerminalController)
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| tool.get("name").and_then(Value::as_str) == Some("execute_command"))
        .collect()
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
            "name": "open_edit_session",
            "description": "Open a write session for the current project workspace. Use one session, stage one or more ordered batches, then finish with commit_edit_session or abort_edit_session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "purpose": { "type": "string" }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "stage_edit_batch",
            "description": "Stage ordered write, replace_text, append, or delete operations without changing project files yet. Multiple operations may target the same file and are applied sequentially to the session snapshot. expected_sha256 is required the first time a path is staged and may be omitted for later operations on that path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "minLength": 1 },
                    "operations": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "kind": {
                                    "type": "string",
                                    "enum": ["write", "replace_text", "append", "delete"]
                                },
                                "path": { "type": "string" },
                                "content": { "type": "string" },
                                "old_text": { "type": "string" },
                                "new_text": { "type": "string" },
                                "start_line": { "type": "integer", "minimum": 1 },
                                "end_line": { "type": "integer", "minimum": 1 },
                                "before_context": { "type": "string" },
                                "after_context": { "type": "string" },
                                "expected_matches": { "type": "integer", "minimum": 1 },
                                "expected_sha256": {
                                    "type": ["string", "null"],
                                    "pattern": "^[0-9a-f]{64}$"
                                }
                            },
                            "additionalProperties": false,
                            "required": ["kind", "path"]
                        }
                    }
                },
                "additionalProperties": false,
                "required": ["session_id", "operations"]
            }
        }),
        json!({
            "name": "commit_edit_session",
            "description": "Atomically commit all staged session changes to the current project workspace. Every touched baseline is revalidated before one multi-file commit request is issued.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false,
                "required": ["session_id"]
            }
        }),
        json!({
            "name": "abort_edit_session",
            "description": "Discard a staged edit session without changing project files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false,
                "required": ["session_id"]
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
        assert!(names.contains(&"open_edit_session"));
        assert!(names.contains(&"stage_edit_batch"));
        assert!(names.contains(&"commit_edit_session"));
        assert!(names.contains(&"abort_edit_session"));
        assert_eq!(names.len(), 8);
    }

    #[test]
    fn terminal_capability_exposes_project_command_execution() {
        let tools = tool_definitions(&HostCapabilityPolicy::from_builtin_kind_names([
            "TerminalController",
        ]));
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["execute_command"]);
    }
}
