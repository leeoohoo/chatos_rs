// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub fn local_command_approval_tool_definitions() -> Vec<Value> {
    vec![local_command_approval_decision_tool_definition()]
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
    }
}
