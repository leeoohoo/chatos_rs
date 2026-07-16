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
            "description": "Persist the current project's provisioned runtime environment, generated environment configuration files, required service images, and connection variables. Prefer provisioning detected runtimes and dependencies; non-runnable is reserved for projects with no executable application or infrastructure entry point. The project id is bound by the server.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["ready", "not_runnable", "failed", "pending_configuration", "pending_image_build"],
                        "description": "Use ready after provisioning detected runtimes and dependency services. Use pending_configuration only for irreducible user-supplied business credentials. Use not_runnable only when the project has no executable entry point or build manifest; missing databases, caches, configuration centers, connection strings, or application configuration must be provisioned or generated instead."
                    },
                    "analysis_summary": {"type": "string"},
                    "not_runnable_reason": {
                        "type": ["string", "null"],
                        "description": "Allowed only when no executable application or infrastructure component can be identified. External service/configuration gaps are not valid reasons."
                    },
                    "detected_stack": {"type": "object"},
                    "required_services": {"type": "array"},
                    "env_vars": {
                        "type": "object",
                        "description": "Legacy flat AI-recommended values. Prefer environment_variables so every value keeps its source."
                    },
                    "environment_variables": {
                        "type": "array",
                        "description": "Environment variables with one effective value derived from a suitable project value or an AI-generated replacement. User-edited effective values are preserved by the server and must not be supplied by the agent.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "project_value": {"type": ["string", "null"]},
                                "project_value_suitable": {
                                    "type": "boolean",
                                    "description": "True only when the value found in the project is directly suitable for the current sandbox."
                                },
                                "recommended_value": {"type": ["string", "null"]},
                                "description": {"type": ["string", "null"]},
                                "recommendation_reason": {"type": ["string", "null"]},
                                "required": {"type": "boolean"},
                                "secret": {"type": "boolean"}
                            },
                            "required": ["name", "project_value_suitable", "required", "secret"],
                            "additionalProperties": false
                        }
                    },
                    "environment_variable_scan": {
                        "type": "object",
                        "description": "Required evidence that the project-wide environment-variable scan was completed before any image provisioning decision.",
                        "properties": {
                            "completed": {"type": "boolean", "const": true},
                            "files_scanned": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "Relevant configuration, manifest, documentation, deployment, and source files inspected during the scan."
                            },
                            "reference_count": {
                                "type": "integer",
                                "minimum": 0,
                                "description": "Number of distinct environment-variable references discovered."
                            },
                            "summary": {
                                "type": "string",
                                "minLength": 1,
                                "description": "Concise evidence of the project-wide scan, including the search scope and whether no references were found."
                            }
                        },
                        "required": ["completed", "files_scanned", "reference_count", "summary"],
                        "additionalProperties": false
                    },
                    "generated_config_files": {
                        "type": "array",
                        "description": "Environment-specific configuration files generated after variable and dependency analysis and before image provisioning. Return an empty array only when the project genuinely needs no generated configuration file. Paths must be workspace-relative and contents should reference environment variables for sensitive or user-editable values.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": {
                                    "type": "string",
                                    "minLength": 1,
                                    "maxLength": 512,
                                    "description": "Workspace-relative target path, for example .env.chatos or src/main/resources/application-chatos.yml."
                                },
                                "format": {
                                    "type": ["string", "null"],
                                    "description": "Configuration format such as dotenv, yaml, json, toml, properties, xml, ini, or text."
                                },
                                "content": {
                                    "type": "string",
                                    "maxLength": 1048576,
                                    "description": "Generated configuration content. Prefer environment-variable placeholders instead of embedding sensitive values."
                                },
                                "description": {"type": ["string", "null"]},
                                "source_files": {
                                    "type": "array",
                                    "items": {"type": "string"},
                                    "description": "Project files and code references used to infer this generated file."
                                }
                            },
                            "required": ["path", "content", "source_files"],
                            "additionalProperties": false
                        }
                    },
                    "last_error": {"type": ["string", "null"]},
                    "images": {
                        "type": "array",
                        "description": "Service plans for one project-level Docker Compose environment. Generate a Dockerfile only for the application runtime; detected databases, caches, and configuration centers are dependency service records that the platform maps to maintained images under the same Compose project.",
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
                                "dockerfile": {
                                    "type": ["string", "null"],
                                    "minLength": 1,
                                    "maxLength": 131072,
                                    "description": "Complete generated Dockerfile for the application runtime. Dependency service records may use null because the platform supplies their maintained images in the project-level Compose file. Do not embed secrets."
                                },
                                "custom_build_script": {
                                    "type": ["string", "null"],
                                    "maxLength": 131072,
                                    "description": "Optional idempotent root Bash installation script equivalent to the generated Dockerfile RUN steps. The current image builder uses this script when the user clicks Generate Image."
                                },
                                "status": {"type": "string"},
                                "error": {"type": ["string", "null"]}
                            },
                            "required": ["environment_key", "environment_type", "display_name", "status"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["environment_variable_scan", "environment_variables", "generated_config_files"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn project_runtime_environment_info_tool_definitions() -> Vec<Value> {
    vec![json!({
        "name": "get_project_runtime_environment_info",
        "description": "Return the current project's initialized runtime environment information, including effective environment variables, generated environment configuration files, dependency services, detected stack, and prepared images. This tool is read-only and the project is bound by the server.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }
    })]
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
        let project_tools = project_environment_tool_definitions();
        assert_eq!(project_tools.len(), 2);
        assert!(project_tools[1]
            .pointer("/inputSchema/properties/environment_variables/items/properties/project_value_suitable")
            .is_some());
        assert_eq!(
            project_tools[1]
                .pointer(
                    "/inputSchema/properties/environment_variable_scan/properties/completed/const"
                )
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            project_tools[1]
                .pointer("/inputSchema/properties/environment_variable_scan/properties/summary/minLength")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert!(project_tools[1]
            .pointer("/inputSchema/properties/generated_config_files/items/properties/content")
            .is_some());
        let runtime_info_tools = project_runtime_environment_info_tool_definitions();
        assert_eq!(runtime_info_tools.len(), 1);
        assert_eq!(
            runtime_info_tools[0].get("name").and_then(Value::as_str),
            Some("get_project_runtime_environment_info")
        );
        assert_eq!(local_command_approval_tool_definitions().len(), 1);
        assert_eq!(
            local_command_approval_decision_tool_definition()["name"].as_str(),
            Some("approval_decision")
        );
    }
}
