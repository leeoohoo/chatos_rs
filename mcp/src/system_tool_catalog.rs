// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub fn local_command_approval_tool_definitions() -> Vec<Value> {
    vec![local_command_approval_decision_tool_definition()]
}

pub fn task_process_log_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "record_process",
            "description": "Record short visible execution breadcrumbs for the current Task Runner task only. Use append for selected approach, root-cause findings, reuse decisions, verification results, blockers, and next steps. Do not record hidden chain-of-thought, credentials, secrets, raw dumps, or unrelated drafts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["append", "replace", "clear"],
                        "default": "append",
                        "description": "append adds one timestamped entry; replace rewrites the full process log; clear removes the process log."
                    },
                    "heading": {
                        "type": ["string", "null"],
                        "description": "Short visible heading for append entries, or null when not needed."
                    },
                    "content": {
                        "type": ["string", "null"],
                        "description": "Visible process content. Required for append/replace; pass null for clear."
                    }
                },
                "required": ["operation", "heading", "content"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "report_outcome",
            "description": "Report the AI's final status for the current Task Runner run. You must call this exactly once, after implementation and verification are finished and immediately before the final user-facing response. The runtime will not accept a final response until this tool has succeeded.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["succeeded", "failed", "blocked"],
                        "description": "succeeded means the requested task is complete; failed means execution finished unsuccessfully; blocked means an external dependency or required input prevents completion."
                    },
                    "reason": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Short concrete reason for the reported status, grounded in the work and verification already performed."
                    }
                }
                ,
                "required": ["status", "reason"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn local_command_approval_decision_tool_definition() -> Value {
    json!({
        "name": "approval_decision",
        "description": "Return the final command approval decision for this request. Must be called exactly once.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "decision": {
                    "type": "string",
                    "enum": ["approve", "deny", "ask_user"]
                },
                "reason": {
                    "type": "string",
                    "description": "Short concrete reason for the decision."
                },
                "remember_allow": {
                    "type": "boolean",
                    "description": "Set true for stable low-risk approve decisions that can be safely whitelisted for repeated identical project commands. Keep false for requested permissions, secrets, destructive operations, project-external paths, or unclear scope."
                }
            },
            "required": ["decision", "reason"],
            "additionalProperties": false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_tool_catalogs_expose_expected_tools() {
        assert_eq!(local_command_approval_tool_definitions().len(), 1);
        assert_eq!(
            local_command_approval_decision_tool_definition()["name"].as_str(),
            Some("approval_decision")
        );
        let process_log_tools = task_process_log_tool_definitions();
        assert_eq!(process_log_tools.len(), 2);
        assert_eq!(
            process_log_tools[0].get("name").and_then(Value::as_str),
            Some("record_process")
        );
        assert_eq!(
            process_log_tools[1].get("name").and_then(Value::as_str),
            Some("report_outcome")
        );
    }
}
