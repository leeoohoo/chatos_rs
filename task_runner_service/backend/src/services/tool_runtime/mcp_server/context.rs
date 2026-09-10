// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    normalize_project_id, CreateTaskRequest, TaskProjectScopeFilter, TaskSourceContext,
    TASK_PROFILE_DEFAULT,
};
use chatos_agent::{parse_chatos_task_runner_tool_profile, ChatosTaskRunnerToolProfile};
use chatos_mcp_runtime::BuiltinMcpPromptLocale;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum McpToolProfile {
    Default,
    ChatosAsyncPlanner,
}

#[derive(Debug, Clone, Default)]
pub struct McpRequestContext {
    pub project_id: Option<String>,
    pub project_context: Option<chatos_mcp_management_sdk::ClientProjectContextSnapshot>,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub remote_connection_id: Option<String>,
    pub default_model_config_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub tool_profile: Option<String>,
    pub task_profile: Option<String>,
    pub builtin_prompt_locale: Option<String>,
}

impl McpRequestContext {
    pub(super) fn task_source_context(&self) -> Result<Option<TaskSourceContext>, String> {
        if self.source_session_id.is_none()
            && self.project_id.is_none()
            && self.project_context.is_none()
            && self.source_turn_id.is_none()
            && self.source_user_message_id.is_none()
            && self.remote_connection_id.is_none()
            && self.workspace_dir.is_none()
        {
            return Ok(None);
        }
        Ok(Some(TaskSourceContext {
            project_id: self.project_id.clone(),
            project_context: self.project_context.clone(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: self.source_session_id.clone(),
            source_turn_id: self.source_turn_id.clone(),
            source_user_message_id: self.source_user_message_id.clone(),
            remote_connection_id: self.remote_connection_id.clone(),
            workspace_dir: self.workspace_dir.clone(),
            builtin_prompt_locale: Some(self.requested_builtin_prompt_locale()),
        }))
    }

    pub(super) fn project_scope_id(&self) -> Option<String> {
        normalize_project_id(self.project_id.clone())
    }

    pub(super) fn project_scope_filter(&self) -> TaskProjectScopeFilter {
        if self.has_concrete_project_scope() {
            TaskProjectScopeFilter::Project
        } else {
            TaskProjectScopeFilter::UserConversation
        }
    }

    pub(super) fn has_concrete_project_scope(&self) -> bool {
        self.project_scope_id().is_some()
    }

    pub(super) fn tool_profile(&self) -> McpToolProfile {
        match self
            .tool_profile
            .as_deref()
            .and_then(parse_chatos_task_runner_tool_profile)
        {
            Some(ChatosTaskRunnerToolProfile::AsyncPlanner) => McpToolProfile::ChatosAsyncPlanner,
            None if self.has_chatos_async_message_context() => McpToolProfile::ChatosAsyncPlanner,
            None => McpToolProfile::Default,
        }
    }

    fn has_chatos_async_message_context(&self) -> bool {
        has_non_empty_text(self.source_session_id.as_deref())
            && has_non_empty_text(self.source_user_message_id.as_deref())
    }

    pub(super) fn requested_task_profile(&self) -> &'static str {
        TASK_PROFILE_DEFAULT
    }

    pub(super) fn enforce_created_task_context(&self, input: &mut CreateTaskRequest) {
        input.project_id = self.project_scope_id();
        input.project_context = self.project_context.clone();
        let has_explicit_model = input
            .default_model_config_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        if !has_explicit_model {
            if let Some(model_config_id) = self
                .default_model_config_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                input.default_model_config_id = Some(model_config_id.to_string());
            }
        }
        input.task_profile = Some(TASK_PROFILE_DEFAULT.to_string());
    }

    pub(super) fn requested_builtin_prompt_locale(&self) -> String {
        let key = match self
            .builtin_prompt_locale
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "en" | "en-us" | "english" => BuiltinMcpPromptLocale::ENGLISH_KEY,
            _ => BuiltinMcpPromptLocale::DEFAULT_KEY,
        };
        key.to_string()
    }
}

fn has_non_empty_text(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}
