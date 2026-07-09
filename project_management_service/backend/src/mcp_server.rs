// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use chatos_project_mcp_contract::{mcp, schemas};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use crate::auth::CurrentUser;
use crate::mcp_tools;
use crate::state::AppState;
use crate::task_runner_api_client;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub server_name: String,
    pub transports: Vec<String>,
    pub http_endpoint_path: String,
    pub tool_names: Vec<String>,
}

pub fn server_info() -> McpServerInfo {
    let tools = tool_definitions();
    let tool_names = tools
        .iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect();
    McpServerInfo {
        server_name: mcp::SERVER_NAME.to_string(),
        transports: vec![mcp::TRANSPORT_HTTP_JSONRPC.to_string()],
        http_endpoint_path: mcp::ENDPOINT_PATH.to_string(),
        tool_names,
    }
}

pub fn tool_definitions() -> Vec<Value> {
    schemas::project_management_server_tool_definitions(None)
}

pub fn tool_definitions_with_execution_options(
    execution_options: Option<&task_runner_api_client::TaskRunnerExecutionOptions>,
) -> Vec<Value> {
    let execution_options =
        execution_options.map(|options| schemas::TaskRunnerExecutionSchemaOptions {
            model_config_ids: options.model_config_ids(),
            default_model_config_id: None,
            tool_ids: options.tool_ids(),
        });
    schemas::project_management_server_tool_definitions(execution_options.as_ref())
}

async fn tool_definitions_for_user(state: &AppState, current_user: &CurrentUser) -> Vec<Value> {
    let Some(owner_user_id) = current_user.effective_owner_user_id() else {
        warn!("project management MCP tools/list cannot enrich Task Runner execution options: missing owner user id");
        return tool_definitions();
    };
    match task_runner_api_client::fetch_execution_options(&state.config, owner_user_id).await {
        Ok(options) => tool_definitions_with_execution_options(Some(&options)),
        Err(err) => {
            warn!(
                owner_user_id,
                error = err.as_str(),
                "project management MCP tools/list failed to fetch Task Runner execution options"
            );
            tool_definitions()
        }
    }
}

pub async fn handle_jsonrpc(
    state: AppState,
    current_user: CurrentUser,
    project_id: Option<String>,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);
    let result = match request.method.as_str() {
        mcp::METHOD_INITIALIZE => Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": mcp::SERVER_NAME,
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {}
            }
        })),
        mcp::METHOD_PING => Ok(json!({})),
        mcp::METHOD_TOOLS_LIST => {
            let tools = tool_definitions_for_user(&state, &current_user).await;
            Ok(json!({ "tools": tools }))
        }
        mcp::METHOD_TOOLS_CALL => {
            let project_id = match project_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(value) => value,
                None => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32000,
                            message: "project management MCP requires current project context"
                                .to_string(),
                        }),
                    };
                }
            };
            mcp_tools::call_tool_from_value(
                &state,
                &current_user,
                project_id,
                request.params.unwrap_or_else(|| json!({})),
            )
            .await
        }
        method => Err(format!("unsupported MCP method: {method}")),
    };
    match result {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        },
        Err(message) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message,
            }),
        },
    }
}

impl From<String> for JsonRpcResponse {
    fn from(message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id: Value::Null,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message,
            }),
        }
    }
}

pub fn jsonrpc_error_response(status: StatusCode, id: Value, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code: if status == StatusCode::UNAUTHORIZED {
                -32001
            } else if status == StatusCode::FORBIDDEN {
                -32003
            } else {
                -32000
            },
            message,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chatos_project_mcp_contract::tools;

    use super::*;
    use crate::domain::visibility::{
        ensure_project_task_queryable_for_mcp, ensure_project_task_status_queryable_for_mcp,
        ensure_requirement_queryable_for_mcp, ensure_requirement_status_queryable_for_mcp,
        non_archived_project_tasks, non_archived_requirements,
    };
    use crate::models::{
        ProjectWorkItemRecord, ProjectWorkItemStatus, RequirementRecord, RequirementStatus,
        RequirementType,
    };

    #[test]
    fn project_scoped_tools_hide_project_context_id() {
        let expected_tools = tools::PROJECT_MANAGEMENT_SERVER_TOOL_NAMES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();

        let tools = tool_definitions();
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        assert_eq!(names, expected_tools);

        for tool in tools {
            let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
            let properties = tool
                .get("inputSchema")
                .and_then(|schema| schema.get("properties"))
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{name} missing properties object"));
            assert!(
                !properties.contains_key("project_id"),
                "{name} must not expose project_id in input schema"
            );
            let required = tool
                .get("inputSchema")
                .and_then(|schema| schema.get("required"))
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("{name} missing required array"));
            assert!(
                !required
                    .iter()
                    .any(|value| value.as_str() == Some("project_id")),
                "{name} must not require project_id"
            );
        }
    }

    #[test]
    fn create_project_task_schema_exposes_task_runner_execution_options() {
        let execution_options = task_runner_api_client::TaskRunnerExecutionOptions::for_test(
            ["model-1"],
            ["CodeMaintainerRead", "TerminalController"],
            ["external-tool-1"],
        );
        let tools = tool_definitions_with_execution_options(Some(&execution_options));
        let create_task = tools
            .iter()
            .find(|tool| {
                tool.get("name").and_then(Value::as_str) == Some(tools::CREATE_PROJECT_TASK)
            })
            .expect("create_project_task tool");
        let properties = create_task
            .pointer("/inputSchema/properties")
            .and_then(Value::as_object)
            .expect("properties");

        assert_eq!(
            properties
                .get("task_runner_default_model_config_id")
                .and_then(|schema| schema.get("enum"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            vec![json!("model-1")]
        );
        let tool_items = properties
            .get("task_runner_enabled_tool_ids")
            .and_then(|schema| schema.get("items"))
            .expect("tool items schema");
        let tool_enum = tool_items
            .get("enum")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(tool_enum.contains(&json!("CodeMaintainerRead")));
        assert!(tool_enum.contains(&json!("TerminalController")));
        assert!(tool_enum.contains(&json!("external-tool-1")));
        assert_eq!(
            properties
                .get("task_runner_enabled_tool_ids")
                .and_then(|schema| schema.get("minItems"))
                .and_then(Value::as_i64),
            Some(1)
        );
        assert!(!properties.contains_key("task_runner_skill_ids"));
        assert!(properties.contains_key("is_planning_task"));
    }

    #[test]
    fn mcp_lists_never_return_archived_items() {
        let requirements = vec![
            requirement_record("req-active", RequirementStatus::Draft),
            requirement_record("req-archived", RequirementStatus::Archived),
        ];
        let visible_requirements = non_archived_requirements(requirements.clone());
        assert_eq!(visible_requirements.len(), 1);
        assert_eq!(visible_requirements[0].id, "req-active");

        let archived_requirements = non_archived_requirements(vec![requirements[1].clone()]);
        assert!(archived_requirements.is_empty());

        let items = vec![
            work_item_record("item-active", ProjectWorkItemStatus::Todo),
            work_item_record("item-archived", ProjectWorkItemStatus::Archived),
        ];
        let visible_items = non_archived_project_tasks(items.clone());
        assert_eq!(visible_items.len(), 1);
        assert_eq!(visible_items[0].id, "item-active");

        let archived_items = non_archived_project_tasks(vec![items[1].clone()]);
        assert!(archived_items.is_empty());
    }

    #[test]
    fn mcp_status_schemas_do_not_advertise_archived() {
        assert!(!schemas::requirement_status_values().contains(&"archived"));
        assert!(!schemas::project_task_status_values().contains(&"archived"));
    }

    #[test]
    fn mcp_rejects_archived_records_by_id() {
        let requirement = requirement_record("req-archived", RequirementStatus::Archived);
        let item = work_item_record("item-archived", ProjectWorkItemStatus::Archived);

        assert_eq!(
            ensure_requirement_queryable_for_mcp(&requirement).unwrap_err(),
            "需求不存在: req-archived"
        );
        assert_eq!(
            ensure_project_task_queryable_for_mcp(&item).unwrap_err(),
            "项目任务不存在: item-archived"
        );
        assert!(
            ensure_requirement_status_queryable_for_mcp(Some(RequirementStatus::Archived)).is_err()
        );
        assert!(ensure_project_task_status_queryable_for_mcp(Some(
            ProjectWorkItemStatus::Archived
        ))
        .is_err());
    }

    fn requirement_record(id: &str, status: RequirementStatus) -> RequirementRecord {
        RequirementRecord {
            id: id.to_string(),
            project_id: "project-1".to_string(),
            parent_requirement_id: None,
            requirement_type: RequirementType::Requirement,
            title: id.to_string(),
            summary: None,
            detail: None,
            business_value: None,
            acceptance_criteria: None,
            source: None,
            priority: 0,
            status,
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            assignee_user_id: None,
            created_at: "2026-06-24T00:00:00.000Z".to_string(),
            updated_at: "2026-06-24T00:00:00.000Z".to_string(),
            archived_at: None,
        }
    }

    fn work_item_record(id: &str, status: ProjectWorkItemStatus) -> ProjectWorkItemRecord {
        ProjectWorkItemRecord {
            id: id.to_string(),
            project_id: "project-1".to_string(),
            requirement_id: "req-active".to_string(),
            title: id.to_string(),
            description: None,
            task_runner_default_model_config_id: "model-config-test".to_string(),
            task_runner_enabled_tool_ids: vec!["filesystem".to_string()],
            task_runner_skill_ids: Vec::new(),
            status,
            priority: 0,
            assignee_user_id: None,
            estimate_points: None,
            due_at: None,
            sort_order: 0,
            tags: Vec::new(),
            is_planning_task: false,
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            created_at: "2026-06-24T00:00:00.000Z".to_string(),
            updated_at: "2026-06-24T00:00:00.000Z".to_string(),
            archived_at: None,
        }
    }
}
