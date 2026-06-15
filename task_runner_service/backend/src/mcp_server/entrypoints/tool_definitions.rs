use super::*;

mod models;
mod prompts;
mod runs;
mod tasks;

impl TaskRunnerMcpService {
    pub fn server_info(&self) -> McpServerInfo {
        McpServerInfo {
            server_name: TASK_RUNNER_MCP_SERVER_NAME.to_string(),
            transports: vec!["http-jsonrpc".to_string(), "stdio-jsonrpc".to_string()],
            http_endpoint_path: Some(TASK_RUNNER_MCP_ENDPOINT_PATH.to_string()),
            stdio_command: Some(TASK_RUNNER_MCP_STDIO_COMMAND.to_string()),
            stdio_args: TASK_RUNNER_MCP_STDIO_ARGS
                .iter()
                .map(|item| item.to_string())
                .collect(),
            tool_names: self
                .list_tools()
                .into_iter()
                .filter_map(|tool| {
                    tool.get("name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .collect(),
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
    ) -> Vec<Value> {
        let mut tools = self.list_tools();
        if let Ok(model_configs) = self.model_config_service.list_model_configs().await {
            enrich_tool_schemas_with_model_configs(&mut tools, &model_configs);
            if tool_profile == McpToolProfile::ChatosAsyncPlanner {
                enrich_tool_schemas_for_async_planner(&mut tools, &model_configs);
            }
        } else if tool_profile == McpToolProfile::ChatosAsyncPlanner {
            enrich_tool_schemas_for_async_planner(&mut tools, &[]);
        }
        if current_user.is_admin() {
            return tools;
        }
        tools
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| agent_tool_allowed_for_profile(name, tool_profile))
            })
            .collect()
    }
}
