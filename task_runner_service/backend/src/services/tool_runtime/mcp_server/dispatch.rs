// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::auth::CurrentUser;

use super::support::agent_tool_allowed_for_request_context;
use super::*;

impl TaskRunnerMcpService {
    pub(in crate::mcp_server) async fn call_tool(
        &self,
        name: &str,
        args: Value,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        if !tool_call_allowed_for_identity(name, current_user.is_admin(), request_context) {
            return Err("当前 agent 无权调用该任务系统工具".to_string());
        }
        match name {
            "list_tasks"
            | "get_task"
            | "get_task_stats"
            | "create_task"
            | "create_tasks_with_prerequisites"
            | "create_project_execution_tasks"
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

fn tool_call_allowed_for_identity(
    name: &str,
    is_admin: bool,
    request_context: &McpRequestContext,
) -> bool {
    agent_tool_allowed_for_request_context(name, request_context)
        || (is_admin && request_context.tool_profile() == McpToolProfile::Default)
}

#[cfg(test)]
mod tests {
    use super::tool_call_allowed_for_identity;
    use crate::mcp_server::{
        McpRequestContext, CHATOS_ASYNC_PLANNER_TOOL_PROFILE,
        PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE,
    };

    #[test]
    fn narrow_tool_profiles_cannot_be_bypassed_by_admin_identity() {
        assert!(tool_call_allowed_for_identity(
            "create_model_config",
            true,
            &McpRequestContext::default(),
        ));
        assert!(tool_call_allowed_for_identity(
            "create_project_execution_tasks",
            true,
            &McpRequestContext {
                tool_profile: Some(PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE.to_string(),),
                ..McpRequestContext::default()
            },
        ));
        assert!(!tool_call_allowed_for_identity(
            "create_task",
            true,
            &McpRequestContext {
                tool_profile: Some(PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE.to_string(),),
                ..McpRequestContext::default()
            },
        ));
        assert!(!tool_call_allowed_for_identity(
            "cancel_task",
            true,
            &McpRequestContext {
                tool_profile: Some(PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE.to_string(),),
                ..McpRequestContext::default()
            },
        ));
        assert!(!tool_call_allowed_for_identity(
            "list_runs",
            false,
            &McpRequestContext {
                tool_profile: Some(CHATOS_ASYNC_PLANNER_TOOL_PROFILE.to_string()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                ..McpRequestContext::default()
            },
        ));
    }
}
