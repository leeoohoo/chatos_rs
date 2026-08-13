// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use crate::models::{
    normalize_project_id, CreateTaskRequest, TaskSourceContext, PUBLIC_PROJECT_ID,
    TASK_PROFILE_CHATOS_PLAN, TASK_PROFILE_DEFAULT,
};
use chatos_agent::{
    is_chatos_plan_task_profile as is_chatos_plan_task_profile_key,
    parse_chatos_task_runner_tool_profile, ChatosTaskRunnerToolProfile,
};
use chatos_mcp_runtime::BuiltinMcpPromptLocale;
use chatos_plugin_management_sdk::TaskPluginConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum McpToolProfile {
    Default,
    ChatosAsyncPlanner,
    ProjectRequirementExecutionPlanner,
}

#[derive(Debug, Clone, Default)]
pub struct McpRequestContext {
    pub project_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub default_model_config_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub tool_profile: Option<String>,
    pub task_profile: Option<String>,
    pub builtin_prompt_locale: Option<String>,
    pub chatos_plan_mode: bool,
    pub expected_project_task_ids: BTreeSet<String>,
    pub plugin_config_override: Option<TaskPluginConfig>,
}

impl McpRequestContext {
    pub(super) fn task_source_context(&self) -> Result<Option<TaskSourceContext>, String> {
        if self.source_session_id.is_none()
            && self.project_id.is_none()
            && self.source_turn_id.is_none()
            && self.source_user_message_id.is_none()
            && self.workspace_dir.is_none()
        {
            return Ok(None);
        }
        Ok(Some(TaskSourceContext {
            project_id: self.project_id.clone(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: self.source_session_id.clone(),
            source_turn_id: self.source_turn_id.clone(),
            source_user_message_id: self.source_user_message_id.clone(),
            workspace_dir: self.workspace_dir.clone(),
            builtin_prompt_locale: Some(self.requested_builtin_prompt_locale()),
        }))
    }

    pub(super) fn project_scope_id(&self) -> Option<String> {
        self.project_id
            .as_ref()
            .map(|value| normalize_project_id(Some(value.clone())))
    }

    pub(super) fn has_concrete_project_scope(&self) -> bool {
        self.project_scope_id()
            .as_deref()
            .is_some_and(|value| value != PUBLIC_PROJECT_ID)
    }

    pub(super) fn tool_profile(&self) -> McpToolProfile {
        match self
            .tool_profile
            .as_deref()
            .and_then(parse_chatos_task_runner_tool_profile)
        {
            Some(ChatosTaskRunnerToolProfile::ProjectRequirementExecutionPlanner) => {
                McpToolProfile::ProjectRequirementExecutionPlanner
            }
            Some(ChatosTaskRunnerToolProfile::AsyncPlanner) => McpToolProfile::ChatosAsyncPlanner,
            None if self.has_chatos_async_message_context() => McpToolProfile::ChatosAsyncPlanner,
            None => McpToolProfile::Default,
        }
    }

    fn has_chatos_async_message_context(&self) -> bool {
        has_non_empty_text(self.source_session_id.as_deref())
            && has_non_empty_text(self.source_user_message_id.as_deref())
    }

    pub(super) fn is_chatos_plan_task_profile(&self) -> bool {
        self.task_profile
            .as_deref()
            .map(str::trim)
            .is_some_and(is_chatos_plan_task_profile_key)
            || self.chatos_plan_mode
    }

    pub(super) fn requested_task_profile(&self) -> &'static str {
        if self.is_chatos_plan_task_profile() {
            TASK_PROFILE_CHATOS_PLAN
        } else {
            TASK_PROFILE_DEFAULT
        }
    }

    pub(super) fn enforce_created_task_kind(&self, input: &mut CreateTaskRequest) {
        if !self.is_chatos_plan_task_profile() {
            input.task_profile = Some(TASK_PROFILE_DEFAULT.to_string());
            return;
        }
        input.task_profile = Some(TASK_PROFILE_CHATOS_PLAN.to_string());
        input
            .mcp_config
            .get_or_insert_with(Default::default)
            .requires_execution = Some(false);
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

    pub(super) fn enforce_plugin_config(&self, input: &mut CreateTaskRequest) {
        if let Some(plugin_config) = self.plugin_config_override.as_ref() {
            input.plugin_config = plugin_config.clone();
        }
    }
}

fn has_non_empty_text(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}
