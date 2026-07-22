// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    normalize_project_id, TaskSourceContext, PUBLIC_PROJECT_ID, TASK_PROFILE_CHATOS_PLAN,
    TASK_PROFILE_DEFAULT,
};
use chatos_mcp_runtime::BuiltinMcpPromptLocale;

use super::{
    decode_remote_server_config_header, CHATOS_ASYNC_PLANNER_TOOL_PROFILE,
    PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE,
};

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
    pub remote_server_config: Option<String>,
    pub tool_profile: Option<String>,
    pub task_profile: Option<String>,
    pub builtin_prompt_locale: Option<String>,
    pub chatos_plan_mode: bool,
}

impl McpRequestContext {
    pub(super) fn task_source_context(&self) -> Result<Option<TaskSourceContext>, String> {
        if self.source_session_id.is_none()
            && self.project_id.is_none()
            && self.source_turn_id.is_none()
            && self.source_user_message_id.is_none()
            && self.workspace_dir.is_none()
            && self.remote_server_config.is_none()
        {
            return Ok(None);
        }
        let remote_server_config = self
            .remote_server_config
            .as_deref()
            .map(decode_remote_server_config_header)
            .transpose()?;
        Ok(Some(TaskSourceContext {
            project_id: self.project_id.clone(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: self.source_session_id.clone(),
            source_turn_id: self.source_turn_id.clone(),
            source_user_message_id: self.source_user_message_id.clone(),
            workspace_dir: self.workspace_dir.clone(),
            remote_server_config,
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
        if self.tool_profile.as_deref().is_some_and(|value| {
            value
                .trim()
                .eq_ignore_ascii_case(PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE)
        }) {
            return McpToolProfile::ProjectRequirementExecutionPlanner;
        }
        if self.tool_profile.as_deref().is_some_and(|value| {
            value
                .trim()
                .eq_ignore_ascii_case(CHATOS_ASYNC_PLANNER_TOOL_PROFILE)
        }) || self.has_chatos_async_message_context()
        {
            McpToolProfile::ChatosAsyncPlanner
        } else {
            McpToolProfile::Default
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
            .is_some_and(|value| value.eq_ignore_ascii_case(TASK_PROFILE_CHATOS_PLAN))
            || self.chatos_plan_mode
    }

    pub(super) fn requested_task_profile(&self) -> &'static str {
        if self.is_chatos_plan_task_profile() {
            TASK_PROFILE_CHATOS_PLAN
        } else {
            TASK_PROFILE_DEFAULT
        }
    }

    pub(super) fn child_task_profile(
        &self,
        is_planning_task: Option<bool>,
        _requires_execution: Option<bool>,
    ) -> Option<String> {
        if !self.is_chatos_plan_task_profile() {
            return None;
        }
        // Preserve the historical planning profile when an older caller omits
        // the field. Planner-facing schemas now require an explicit value.
        let is_planning_task = is_planning_task.unwrap_or(true);
        Some(
            if is_planning_task {
                TASK_PROFILE_CHATOS_PLAN
            } else {
                crate::models::TASK_PROFILE_DEFAULT
            }
            .to_string(),
        )
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
