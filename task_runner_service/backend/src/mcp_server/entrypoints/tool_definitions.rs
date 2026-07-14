// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod models;
mod prompts;
mod runs;
mod tasks;

impl TaskRunnerMcpService {
    pub fn provider_descriptor(&self) -> McpProviderDescriptor {
        let mut tools = self
            .list_tools()
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        agent_tool_allowed_for_profile(name, McpToolProfile::ChatosAsyncPlanner)
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
            server_name: TASK_RUNNER_MCP_SERVER_NAME.to_string(),
            skills: vec![McpProviderSkill {
                id: "task_runner_usage".to_string(),
                name: "Task Runner MCP 使用指南".to_string(),
                description: "指导 AI 把当前用户和项目需求交给内部异步执行链路，并正确选择 MCP 与 Local Connector Skills。".to_string(),
                instructions: include_str!(
                    "../../../../mcp/task-runner-provider-skill.md"
                )
                .to_string(),
            }],
            tools,
        }
    }

    pub fn server_info(&self) -> McpServerInfo {
        let tools = self.list_tools();
        let tool_names = tool_names_from_tools(&tools);
        McpServerInfo {
            server_name: TASK_RUNNER_MCP_SERVER_NAME.to_string(),
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
        tool_profile: McpToolProfile,
    ) -> Result<Vec<Value>, String> {
        let mut tools = self.list_tools();
        if let Ok(model_configs) = self.model_config_service.list_model_configs().await {
            let visible_model_configs = filter_model_configs_for_user(model_configs, current_user);
            enrich_tool_schemas_with_model_configs(&mut tools, &visible_model_configs);
            if tool_profile == McpToolProfile::ChatosAsyncPlanner {
                enrich_tool_schemas_for_async_planner(&mut tools, &visible_model_configs);
            }
        } else if tool_profile == McpToolProfile::ChatosAsyncPlanner {
            enrich_tool_schemas_for_async_planner(&mut tools, &[]);
        }
        if current_user.is_admin() && tool_profile == McpToolProfile::Default {
            return Ok(tools);
        }
        let owner_user_id = current_user
            .effective_owner_user_id()
            .ok_or_else(|| "current agent token is missing owner scope".to_string())?;
        if let Some(policy) = self
            .task_service
            .resolve_task_runner_policy(Some(current_user), Some(owner_user_id))
            .await?
        {
            restrict_task_capability_selection_schemas(
                &mut tools,
                policy.selectable_builtin_kind_names().as_slice(),
                policy.selectable_external_mcp_ids().as_slice(),
                policy.selectable_skill_ids().as_slice(),
            );
        }
        Ok(tools
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| agent_tool_allowed_for_profile(name, tool_profile))
            })
            .collect())
    }
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
