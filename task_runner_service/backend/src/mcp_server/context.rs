use crate::models::TaskSourceContext;

use super::{decode_remote_server_config_header, CHATOS_ASYNC_PLANNER_TOOL_PROFILE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum McpToolProfile {
    Default,
    ChatosAsyncPlanner,
}

#[derive(Debug, Clone, Default)]
pub struct McpRequestContext {
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub remote_server_config: Option<String>,
    pub tool_profile: Option<String>,
}

impl McpRequestContext {
    pub(super) fn task_source_context(&self) -> Result<Option<TaskSourceContext>, String> {
        if self.source_session_id.is_none()
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
            parent_task_id: None,
            source_run_id: None,
            source_session_id: self.source_session_id.clone(),
            source_turn_id: self.source_turn_id.clone(),
            source_user_message_id: self.source_user_message_id.clone(),
            workspace_dir: self.workspace_dir.clone(),
            remote_server_config,
        }))
    }

    pub(super) fn tool_profile(&self) -> McpToolProfile {
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
}

fn has_non_empty_text(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}
