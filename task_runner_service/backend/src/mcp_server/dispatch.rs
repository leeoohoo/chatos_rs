use serde_json::Value;

use crate::auth::CurrentUser;

use super::support::agent_tool_allowed_for_profile;
use super::*;

impl TaskRunnerMcpService {
    pub(in crate::mcp_server) async fn call_tool(
        &self,
        name: &str,
        args: Value,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        if !current_user.is_admin()
            && !agent_tool_allowed_for_profile(name, request_context.tool_profile())
        {
            return Err("当前 agent 无权调用该任务系统工具".to_string());
        }
        match name {
            "list_tasks"
            | "get_task"
            | "get_task_stats"
            | "create_task"
            | "list_mcp_builtin_catalog"
            | "list_external_mcp_configs"
            | "create_tasks_with_prerequisites"
            | "update_task"
            | "set_task_prerequisites"
            | "cancel_task"
            | "wait_for_task_completion"
            | "get_task_dependency_graph"
            | "delete_task"
            | "batch_update_task_status"
            | "batch_delete_tasks" => {
                self.call_task_tool(name, args, current_user, request_context)
                    .await
            }
            "list_model_configs"
            | "get_model_config"
            | "create_model_config"
            | "update_model_config"
            | "delete_model_config"
            | "test_model_config" => self.call_model_tool(name, args, current_user).await,
            "list_runs"
            | "get_run"
            | "start_task_run"
            | "batch_start_task_runs"
            | "get_task_memory_context"
            | "list_task_memory_records"
            | "summarize_task_memory"
            | "cancel_run"
            | "retry_run"
            | "list_run_events" => {
                self.call_run_tool(name, args, current_user, request_context)
                    .await
            }
            "list_prompts" | "get_prompt" | "submit_prompt" | "cancel_prompt" => {
                self.call_prompt_tool(name, args, current_user, request_context)
                    .await
            }
            other => Err(format!("tool not found: {other}")),
        }
    }
}
