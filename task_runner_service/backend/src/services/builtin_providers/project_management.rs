// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

#[cfg(test)]
use chatos_mcp::project_management_contract::schemas;
use chatos_mcp::project_management_contract::{mcp, tools};
use serde_json::{json, Value};

use crate::models::{normalize_project_id, PUBLIC_PROJECT_ID};

#[derive(Clone)]
pub(in crate::services) struct ProjectManagementBuiltinService {
    server_name: String,
    base_url: Option<String>,
    sync_secret: Option<String>,
    owner_user_id: Option<String>,
    project_id: Option<String>,
}

impl ProjectManagementBuiltinService {
    pub(in crate::services) fn new(options: ProjectManagementOptions) -> Self {
        Self {
            server_name: options.server_name,
            base_url: normalize_optional(options.base_url),
            sync_secret: normalize_optional(options.sync_secret),
            owner_user_id: normalize_optional(options.owner_user_id),
            project_id: normalize_optional(options.project_id),
        }
    }

    pub(in crate::services) fn list_tools(&self) -> Vec<Value> {
        tool_definitions()
    }

    pub(in crate::services) async fn call_tool(
        &self,
        name: &str,
        args: Value,
    ) -> Result<Value, String> {
        if let Some(result) = archived_status_short_circuit(name, &args)? {
            return Ok(result);
        }
        let base_url = self
            .base_url
            .as_deref()
            .ok_or_else(|| "project service base url is not configured".to_string())?;
        let sync_secret = self
            .sync_secret
            .as_deref()
            .ok_or_else(|| "project service sync secret is not configured".to_string())?;
        let owner_user_id = self.owner_user_id.as_deref().ok_or_else(|| {
            format!(
                "{} builtin missing owner user id",
                self.server_name.as_str()
            )
        })?;
        let project_id = normalize_project_id(self.project_id.clone());
        if project_id == PUBLIC_PROJECT_ID {
            return Err(format!(
                "{} builtin requires concrete project_id",
                self.server_name.as_str()
            ));
        }

        let mut headers = HashMap::new();
        super::super::project_management_api_client::insert_project_service_mcp_signing_headers(
            &mut headers,
            sync_secret,
            super::super::project_management_api_client::PROJECT_MCP_SCOPE,
        )?;
        headers.insert(
            "X-Task-Runner-Owner-User-Id".to_string(),
            owner_user_id.to_string(),
        );
        headers.insert("X-Chatos-Project-Id".to_string(), project_id);
        headers.insert(
            "X-Task-Runner-Task-Profile".to_string(),
            crate::models::TASK_PROFILE_CHATOS_PLAN.to_string(),
        );
        let result = chatos_mcp_runtime::jsonrpc_http_call(
            format!("{}{}", base_url.trim_end_matches('/'), mcp::ENDPOINT_PATH).as_str(),
            Some(&headers),
            mcp::METHOD_TOOLS_CALL,
            json!({
                "name": name,
                "arguments": args,
            }),
            None,
        )
        .await?;
        filter_archived_tool_result(name, result)
    }

    pub(in crate::services) fn unavailable_tools(&self) -> Vec<(String, String)> {
        let project_id = normalize_project_id(self.project_id.clone());
        let reason = if self.base_url.is_none() {
            Some("project service base url is not configured")
        } else if self.sync_secret.is_none() {
            Some("project service sync secret is not configured")
        } else if self.owner_user_id.is_none() {
            Some("project management builtin missing owner user id")
        } else if project_id == PUBLIC_PROJECT_ID {
            Some("project management builtin requires concrete project_id")
        } else {
            None
        };
        let Some(reason) = reason else {
            return Vec::new();
        };
        tool_definitions()
            .into_iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(|name| (name.to_string(), reason.to_string()))
            })
            .collect()
    }
}

#[derive(Clone)]
pub(in crate::services) struct ProjectManagementOptions {
    pub(in crate::services) server_name: String,
    pub(in crate::services) base_url: Option<String>,
    pub(in crate::services) sync_secret: Option<String>,
    pub(in crate::services) owner_user_id: Option<String>,
    pub(in crate::services) project_id: Option<String>,
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn tool_definitions() -> Vec<Value> {
    chatos_mcp::system_mcp_static_tools(
        chatos_plugin_management_sdk::SystemMcpKey::ProjectManagement,
    )
    .expect("Project Management must have a static system MCP catalog")
}

fn archived_status_short_circuit(name: &str, args: &Value) -> Result<Option<Value>, String> {
    let status = args.get("status").and_then(Value::as_str);
    let patch_status = args
        .get("patch")
        .and_then(Value::as_object)
        .and_then(|patch| patch.get("status"))
        .and_then(Value::as_str);
    let has_archived_status = status == Some("archived") || patch_status == Some("archived");
    if !has_archived_status {
        return Ok(None);
    }

    match name {
        tools::LIST_REQUIREMENTS | tools::LIST_PROJECT_TASKS => {
            Ok(Some(tool_text_result(json!([]))))
        }
        tools::CREATE_REQUIREMENT | tools::UPDATE_REQUIREMENT => {
            Err("Project Management MCP 不允许访问归档需求".to_string())
        }
        tools::CREATE_PROJECT_TASK | tools::UPDATE_PROJECT_TASK => {
            Err("Project Management MCP 不允许访问归档项目任务".to_string())
        }
        _ => Ok(None),
    }
}

fn filter_archived_tool_result(name: &str, result: Value) -> Result<Value, String> {
    match name {
        tools::LIST_REQUIREMENTS | tools::LIST_PROJECT_TASKS => {
            transform_tool_text_payload(result, filter_archived_array)
        }
        tools::GET_PROJECT_DEPENDENCY_GRAPH => {
            transform_tool_text_payload(result, filter_archived_dependency_graph)
        }
        _ => Ok(result),
    }
}

fn transform_tool_text_payload(
    mut result: Value,
    transform: fn(Value) -> Value,
) -> Result<Value, String> {
    let Some(content) = result.get_mut("content").and_then(Value::as_array_mut) else {
        return Ok(result);
    };
    for item in content {
        if item.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let Some(text) = item.get("text").and_then(Value::as_str) else {
            continue;
        };
        let Ok(payload) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        let filtered = transform(payload);
        item["text"] = Value::String(
            serde_json::to_string_pretty(&filtered).unwrap_or_else(|_| filtered.to_string()),
        );
        break;
    }
    Ok(result)
}

fn filter_archived_array(payload: Value) -> Value {
    let Value::Array(items) = payload else {
        return payload;
    };
    Value::Array(
        items
            .into_iter()
            .filter(|item| item.get("status").and_then(Value::as_str) != Some("archived"))
            .collect(),
    )
}

fn filter_archived_dependency_graph(mut payload: Value) -> Value {
    let Some(object) = payload.as_object_mut() else {
        return payload;
    };
    let nodes = object
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut visible_requirement_ids = HashSet::new();
    for node in &nodes {
        if node.get("status").and_then(Value::as_str) == Some("archived") {
            continue;
        }
        if node.get("node_type").and_then(Value::as_str) == Some("requirement") {
            if let Some(raw_id) = node.get("raw_id").and_then(Value::as_str) {
                visible_requirement_ids.insert(raw_id.to_string());
            }
        }
    }

    let filtered_nodes = nodes
        .into_iter()
        .filter(|node| {
            if node.get("status").and_then(Value::as_str) == Some("archived") {
                return false;
            }
            if node.get("node_type").and_then(Value::as_str) == Some("work_item") {
                return node
                    .get("parent_id")
                    .and_then(Value::as_str)
                    .is_some_and(|parent_id| visible_requirement_ids.contains(parent_id));
            }
            true
        })
        .collect::<Vec<_>>();
    let visible_node_ids = filtered_nodes
        .iter()
        .filter_map(|node| {
            node.get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<HashSet<_>>();
    object.insert("nodes".to_string(), Value::Array(filtered_nodes));

    if let Some(edges) = object.get("edges").and_then(Value::as_array).cloned() {
        object.insert(
            "edges".to_string(),
            Value::Array(
                edges
                    .into_iter()
                    .filter(|edge| {
                        let from_visible = edge
                            .get("from")
                            .and_then(Value::as_str)
                            .is_some_and(|id| visible_node_ids.contains(id));
                        let to_visible = edge
                            .get("to")
                            .and_then(Value::as_str)
                            .is_some_and(|id| visible_node_ids.contains(id));
                        from_visible && to_visible
                    })
                    .collect(),
            ),
        );
    }
    payload
}

fn tool_text_result(payload: Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
            }
        ],
        "isError": false
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn builtin_tool_names_match_contract() {
        let expected = tools::TASK_RUNNER_BUILTIN_TOOL_NAMES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let definitions = tool_definitions();
        let actual = definitions
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn create_project_task_schema_excludes_execution_options() {
        let definitions = tool_definitions();
        let create_task = definitions
            .iter()
            .find(|tool| {
                tool.get("name").and_then(Value::as_str) == Some(tools::CREATE_PROJECT_TASK)
            })
            .expect("create_project_task tool");
        let properties = create_task
            .pointer("/inputSchema/properties")
            .and_then(Value::as_object)
            .expect("properties");

        assert!(!properties.contains_key("task_runner_default_model_config_id"));
        assert!(!properties.contains_key("task_runner_enabled_tool_ids"));
    }

    #[test]
    fn status_schemas_do_not_advertise_archived() {
        assert!(!schemas::requirement_status_values().contains(&"archived"));
        assert!(!schemas::project_task_status_values().contains(&"archived"));
    }

    #[test]
    fn archived_status_queries_are_short_circuited() {
        let result = archived_status_short_circuit(
            tools::LIST_REQUIREMENTS,
            &json!({ "status": "archived" }),
        )
        .expect("short circuit")
        .expect("empty result");
        let text = result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .expect("text result");
        assert_eq!(serde_json::from_str::<Value>(text).unwrap(), json!([]));

        assert!(archived_status_short_circuit(
            tools::UPDATE_PROJECT_TASK,
            &json!({ "patch": { "status": "archived" } }),
        )
        .is_err());
    }

    #[test]
    fn dependency_graph_filter_removes_archived_nodes_and_edges() {
        let graph = json!({
            "root_id": "project:project-1",
            "nodes": [
                {
                    "id": "requirement:req-visible",
                    "node_type": "requirement",
                    "raw_id": "req-visible",
                    "status": "approved"
                },
                {
                    "id": "requirement:req-archived",
                    "node_type": "requirement",
                    "raw_id": "req-archived",
                    "status": "archived"
                },
                {
                    "id": "work_item:item-visible",
                    "node_type": "work_item",
                    "raw_id": "item-visible",
                    "status": "todo",
                    "parent_id": "req-visible"
                },
                {
                    "id": "work_item:item-under-archived",
                    "node_type": "work_item",
                    "raw_id": "item-under-archived",
                    "status": "todo",
                    "parent_id": "req-archived"
                },
                {
                    "id": "work_item:item-archived",
                    "node_type": "work_item",
                    "raw_id": "item-archived",
                    "status": "archived",
                    "parent_id": "req-visible"
                }
            ],
            "edges": [
                {
                    "from": "requirement:req-visible",
                    "to": "work_item:item-visible",
                    "edge_type": "contains"
                },
                {
                    "from": "requirement:req-archived",
                    "to": "work_item:item-under-archived",
                    "edge_type": "contains"
                },
                {
                    "from": "work_item:item-visible",
                    "to": "work_item:item-archived",
                    "edge_type": "blocks"
                }
            ]
        });

        let filtered = filter_archived_dependency_graph(graph);
        let nodes = filtered
            .get("nodes")
            .and_then(Value::as_array)
            .expect("nodes");
        let node_ids = nodes
            .iter()
            .filter_map(|node| node.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(
            node_ids,
            vec!["requirement:req-visible", "work_item:item-visible"]
        );

        let edges = filtered
            .get("edges")
            .and_then(Value::as_array)
            .expect("edges");
        assert_eq!(edges.len(), 1);
        assert_eq!(
            edges[0].get("to").and_then(Value::as_str),
            Some("work_item:item-visible")
        );
    }
}
