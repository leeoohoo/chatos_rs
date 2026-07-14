// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub fn project_environment_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "get_current_project_runtime_environment",
            "description": "Get the current project details and persisted runtime environment for this project. The project id is bound by the server.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "update_current_project_runtime_environment",
            "description": "Persist the current project's provisioned runtime environment, required service images, and generated connection variables. Prefer provisioning detected runtimes and dependencies; non-runnable is reserved for projects with no executable application or infrastructure entry point. The project id is bound by the server.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["ready", "not_runnable", "failed", "pending_configuration"],
                        "description": "Use ready after provisioning detected runtimes and dependency services. Use pending_configuration only for irreducible user-supplied business credentials. Use not_runnable only when the project has no executable entry point or build manifest; missing databases, caches, configuration centers, connection strings, or application configuration must be provisioned or generated instead."
                    },
                    "analysis_summary": {"type": "string"},
                    "not_runnable_reason": {
                        "type": ["string", "null"],
                        "description": "Allowed only when no executable application or infrastructure component can be identified. External service/configuration gaps are not valid reasons."
                    },
                    "detected_stack": {"type": "object"},
                    "required_services": {"type": "array"},
                    "env_vars": {"type": "object"},
                    "last_error": {"type": ["string", "null"]},
                    "images": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "environment_key": {"type": "string"},
                                "environment_type": {"type": "string"},
                                "display_name": {"type": "string"},
                                "image_id": {"type": ["string", "null"]},
                                "image_ref": {"type": ["string", "null"]},
                                "image_provider": {"type": "string"},
                                "features": {"type": "array"},
                                "ports": {"type": "array"},
                                "env_vars": {"type": "object"},
                                "status": {"type": "string"},
                                "error": {"type": ["string", "null"]}
                            },
                            "required": ["environment_key", "environment_type", "display_name", "status"],
                            "additionalProperties": false
                        }
                    }
                },
                "additionalProperties": false
            }
        }),
    ]
}

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
                    "description": "Set true only for a stable low-risk approve decision that should be whitelisted."
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
    fn system_routed_catalogs_expose_expected_tools() {
        assert_eq!(project_environment_tool_definitions().len(), 2);
        assert_eq!(local_command_approval_tool_definitions().len(), 1);
        assert_eq!(
            local_command_approval_decision_tool_definition()["name"].as_str(),
            Some("approval_decision")
        );
    }
}
