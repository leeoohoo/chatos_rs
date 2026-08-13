// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use super::*;

#[path = "tool_definitions/models.rs"]
mod models;
#[path = "tool_definitions/prompts.rs"]
mod prompts;
#[path = "tool_definitions/runs.rs"]
mod runs;
#[path = "tool_definitions/tasks.rs"]
mod tasks;

impl TaskRunnerMcpService {
    pub fn provider_descriptor(&self) -> McpProviderDescriptor {
        let system_descriptor = chatos_mcp::system_mcp_descriptor(
            chatos_plugin_management_sdk::SystemMcpKey::TaskRunnerService,
        );
        let skills = chatos_mcp::system_mcp_provider_skills(system_descriptor.key)
            .into_iter()
            .map(|skill| McpProviderSkill {
                id: skill.id,
                name: skill.name,
                description: skill.description,
                instructions: skill.instructions,
            })
            .collect();
        let mut tools = self
            .list_tools()
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        // This descriptor is the control-plane catalog shared by
                        // every ChatOS Agent binding. The request-scoped Task
                        // Runner endpoint still applies the exact tool profile,
                        // but the catalog must contain the union of tools that
                        // any supported ChatOS planner profile can receive. If
                        // it only advertises the ordinary async-planner subset,
                        // MCP Management cannot grant the requirement execution
                        // planner its dedicated materialization tool.
                        agent_tool_allowed_for_profile(name, McpToolProfile::ChatosAsyncPlanner)
                            || agent_tool_allowed_for_profile(
                                name,
                                McpToolProfile::ProjectRequirementExecutionPlanner,
                            )
                    })
            })
            .collect::<Vec<_>>();
        for tool in &mut tools {
            if tool.get("outputSchema").is_none() {
                tool["outputSchema"] = json!({
                    "type": "object",
                    "description": "Structured JSON result returned by this Task Runner tool. Exact fields depend on the operation and are also returned through the standard MCP content envelope.",
                    "additionalProperties": true
                });
            }
        }
        McpProviderDescriptor {
            server_name: system_descriptor.server_name.to_string(),
            skills,
            tools,
        }
    }

    pub fn server_info(&self) -> McpServerInfo {
        let system_descriptor = chatos_mcp::system_mcp_descriptor(
            chatos_plugin_management_sdk::SystemMcpKey::TaskRunnerService,
        );
        let tools = self.list_tools();
        let tool_names = tool_names_from_tools(&tools);
        McpServerInfo {
            server_name: system_descriptor.server_name.to_string(),
            transports: vec!["http-jsonrpc".to_string(), "stdio-jsonrpc".to_string()],
            http_endpoint_path: Some(TASK_RUNNER_MCP_ENDPOINT_PATH.to_string()),
            stdio_command: Some(TASK_RUNNER_MCP_STDIO_COMMAND.to_string()),
            stdio_args: TASK_RUNNER_MCP_STDIO_ARGS
                .iter()
                .map(|item| item.to_string())
                .collect(),
            tool_names: tool_names.clone(),
            tool_profiles: vec![
                McpServerToolProfileInfo {
                    key: "admin_full".to_string(),
                    label: "Admin / full metadata".to_string(),
                    description:
                        "Complete server metadata list before user/profile access filtering."
                            .to_string(),
                    tool_names: tool_names.clone(),
                },
                McpServerToolProfileInfo {
                    key: "agent_default".to_string(),
                    label: "Agent default".to_string(),
                    description: "Default non-admin agent allowlist.".to_string(),
                    tool_names: tool_names_for_profile(&tools, McpToolProfile::Default),
                },
                McpServerToolProfileInfo {
                    key: CHATOS_ASYNC_PLANNER_TOOL_PROFILE.to_string(),
                    label: "Chatos async planner".to_string(),
                    description: "Narrow allowlist used by Chatos async message planning."
                        .to_string(),
                    tool_names: tool_names_for_profile(&tools, McpToolProfile::ChatosAsyncPlanner),
                },
                McpServerToolProfileInfo {
                    key: PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE.to_string(),
                    label: "Project requirement execution planner".to_string(),
                    description: "Tools used by Chatos to split project tasks into concrete Task Runner execution tasks.".to_string(),
                    tool_names: tool_names_for_profile(
                        &tools,
                        McpToolProfile::ProjectRequirementExecutionPlanner,
                    ),
                },
            ],
        }
    }

    pub fn list_tools(&self) -> Vec<Value> {
        let mut tools = tasks::task_tool_definitions();
        tools.extend(models::model_tool_definitions());
        tools.extend(runs::run_tool_definitions());
        tools.extend(prompts::prompt_tool_definitions());
        tools
    }

    pub(super) async fn list_tools_for_user(
        &self,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Vec<Value>, String> {
        let tool_profile = request_context.tool_profile();
        let mut tools = self.list_tools();
        match self.model_config_service.list_model_configs().await {
            Ok(model_configs) => {
                let visible_model_configs =
                    filter_model_configs_for_user(model_configs, current_user);
                enrich_tool_schemas_with_model_configs(&mut tools, &visible_model_configs);
                if tool_profile == McpToolProfile::ChatosAsyncPlanner {
                    enrich_tool_schemas_for_async_planner(&mut tools, &visible_model_configs);
                }
            }
            Err(err) => {
                tracing::warn!(
                    error = err.as_str(),
                    "task runner could not enrich MCP tool schemas with model configs"
                );
                if tool_profile == McpToolProfile::ChatosAsyncPlanner {
                    enrich_tool_schemas_for_async_planner(&mut tools, &[]);
                }
            }
        }
        if tool_profile != McpToolProfile::ChatosAsyncPlanner {
            match self
                .task_mcp_schema_choices(current_user, request_context)
                .await
            {
                Ok((builtin_choices, external_choices)) => {
                    enrich_tool_schemas_with_task_mcp_choices(
                        &mut tools,
                        builtin_choices.as_slice(),
                        external_choices.as_slice(),
                    );
                }
                Err(err) => tracing::warn!(
                    error = err.as_str(),
                    "task runner could not enrich MCP tool schemas with Agent-bound MCP choices"
                ),
            }
        }
        Ok(tools
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        super::super::support::agent_tool_allowed_for_request_context(
                            name,
                            request_context,
                        )
                    })
            })
            .collect())
    }

    async fn task_mcp_schema_choices(
        &self,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<(Vec<TaskMcpSchemaChoice>, Vec<TaskMcpSchemaChoice>), String> {
        let owner_user_id = current_user
            .effective_owner_user_id()
            .ok_or_else(|| "current Agent is missing owner scope".to_string())?;
        let project_id = request_context
            .project_scope_id()
            .unwrap_or_else(|| crate::models::PUBLIC_PROJECT_ID.to_string());
        let targets = [
            (crate::models::TASK_PROFILE_DEFAULT, true, "execution task"),
            (
                crate::models::TASK_PROFILE_CHATOS_PLAN,
                false,
                "planning task",
            ),
            (
                crate::models::TASK_PROFILE_CHATOS_PLAN,
                true,
                "planning-profile execution task",
            ),
        ];
        let mut builtin = BTreeMap::<String, String>::new();
        let mut external = BTreeMap::<String, String>::new();
        for (task_profile, requires_execution, target_label) in targets {
            let agent_key =
                crate::models::task_runner_agent_key_for(task_profile, requires_execution);
            let Some(policy) = self
                .task_service
                .resolve_task_runner_policy_for_agent_project(
                    Some(current_user),
                    Some(owner_user_id),
                    agent_key,
                    project_id.as_str(),
                    Some(task_profile),
                    None,
                )
                .await?
            else {
                continue;
            };
            for (value, title) in policy.selectable_builtin_mcp_choices() {
                merge_mcp_choice(&mut builtin, value, title, target_label);
            }
            for (value, title) in policy.selectable_external_mcp_choices() {
                merge_mcp_choice(&mut external, value, title, target_label);
            }
        }
        Ok((
            builtin
                .into_iter()
                .map(|(value, title)| TaskMcpSchemaChoice { value, title })
                .collect(),
            external
                .into_iter()
                .map(|(value, title)| TaskMcpSchemaChoice { value, title })
                .collect(),
        ))
    }
}

fn merge_mcp_choice(
    choices: &mut BTreeMap<String, String>,
    value: String,
    title: String,
    target_label: &str,
) {
    let labeled = format!("{title} [{target_label}]");
    choices
        .entry(value)
        .and_modify(|existing| {
            if !existing.contains(target_label) {
                existing.push_str("; ");
                existing.push_str(labeled.as_str());
            }
        })
        .or_insert(labeled);
}

fn tool_names_from_tools(tools: &[Value]) -> Vec<String> {
    tools.iter().filter_map(tool_name).collect()
}

fn tool_names_for_profile(tools: &[Value], tool_profile: McpToolProfile) -> Vec<String> {
    tools
        .iter()
        .filter_map(|tool| {
            let name = tool_name(tool)?;
            agent_tool_allowed_for_profile(&name, tool_profile).then_some(name)
        })
        .collect()
}

fn tool_name(tool: &Value) -> Option<String> {
    tool.get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}
