// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use std::collections::BTreeMap;

pub const TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC: &str = "project_service_sync";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEphemeralHttpMcpServer {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub auth_mode: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TaskStatus {
    #[default]
    Draft,
    Ready,
    Queued,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    Archived,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskManagerScope {
    RunChecklist,
    DurableFollowup,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskClosureState {
    Open,
    Satisfied,
    BlockedTerminal,
    Cancelled,
    Superseded,
    Waived,
    Orphaned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TaskProcessLogOperation {
    #[default]
    Append,
    Replace,
    Clear,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskReportedOutcomeStatus {
    Succeeded,
    Failed,
    Blocked,
}

impl TaskReportedOutcomeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMcpConfig {
    #[serde(default = "task_mcp_enabled_default")]
    pub enabled: bool,
    #[serde(default)]
    pub init_mode: TaskMcpInitMode,
    #[serde(default)]
    pub builtin_prompt_mode: TaskBuiltinMcpPromptMode,
    #[serde(default = "task_mcp_locale_default")]
    pub builtin_prompt_locale: String,
    #[serde(default = "task_mcp_builtin_kinds_default")]
    pub enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    pub workspace_dir: Option<String>,
    #[serde(default = "task_requires_execution_default")]
    pub requires_execution: bool,
    #[serde(default = "task_workspace_changes_required_default")]
    pub workspace_changes_required: bool,
    #[serde(default)]
    pub execution_service_id: Option<String>,
    #[serde(default)]
    pub external_mcp_config_ids: Vec<String>,
    #[serde(default)]
    pub selected_skill_ids: Vec<String>,
    #[serde(default)]
    pub skill_policy_revision: Option<String>,
    #[serde(default)]
    pub ephemeral_http_servers: Vec<TaskEphemeralHttpMcpServer>,
}

impl Default for TaskMcpConfig {
    fn default() -> Self {
        Self {
            enabled: task_mcp_enabled_default(),
            init_mode: TaskMcpInitMode::Full,
            builtin_prompt_mode: TaskBuiltinMcpPromptMode::Effective,
            builtin_prompt_locale: task_mcp_locale_default(),
            enabled_builtin_kinds: task_mcp_builtin_kinds_default(),
            workspace_dir: None,
            requires_execution: task_requires_execution_default(),
            workspace_changes_required: task_workspace_changes_required_default(),
            execution_service_id: None,
            external_mcp_config_ids: Vec::new(),
            selected_skill_ids: Vec::new(),
            skill_policy_revision: None,
            ephemeral_http_servers: Vec::new(),
        }
    }
}

impl TaskMcpConfig {
    pub fn locale(&self) -> BuiltinMcpPromptLocale {
        BuiltinMcpPromptLocale::from_key(Some(&self.builtin_prompt_locale))
    }
}

fn task_mcp_enabled_default() -> bool {
    true
}

fn task_requires_execution_default() -> bool {
    true
}

fn task_workspace_changes_required_default() -> bool {
    true
}

fn task_mcp_locale_default() -> String {
    BuiltinMcpPromptLocale::DEFAULT_KEY.to_string()
}

fn task_mcp_builtin_kinds_default() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod task_mcp_config_tests {
    use super::*;

    #[test]
    fn legacy_task_config_defaults_to_requiring_execution() {
        let config = serde_json::from_value::<TaskMcpConfig>(serde_json::json!({}))
            .expect("legacy task config");
        assert!(config.requires_execution);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TaskScheduleMode {
    #[default]
    Manual,
    Once,
    Interval,
    ContactAsync,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskScheduleConfig {
    #[serde(default)]
    pub mode: TaskScheduleMode,
    #[serde(default)]
    pub run_at: Option<String>,
    #[serde(default)]
    pub interval_seconds: Option<i64>,
    #[serde(default)]
    pub next_run_at: Option<String>,
    #[serde(default)]
    pub last_scheduled_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskToolOutcomeItem {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub importance: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskToolState {
    #[serde(default)]
    pub execution_paused: bool,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub outcome_items: Vec<TaskToolOutcomeItem>,
    #[serde(default)]
    pub resume_hint: Option<String>,
    #[serde(default)]
    pub blocker_reason: Option<String>,
    #[serde(default)]
    pub blocker_needs: Vec<String>,
    #[serde(default)]
    pub blocker_kind: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub last_outcome_at: Option<String>,
    #[serde(default)]
    pub cancel_reason: Option<String>,
    #[serde(default)]
    pub cancelled_at: Option<String>,
    #[serde(default)]
    pub cancelled_by_user_id: Option<String>,
    #[serde(default)]
    pub cancelled_by_username: Option<String>,
    #[serde(default)]
    pub cancelled_by_display_name: Option<String>,
    #[serde(default)]
    pub replacement_task_ids: Vec<String>,
    #[serde(default)]
    pub cancelled_because_task_id: Option<String>,
    #[serde(default)]
    pub cascade_root_task_id: Option<String>,
    #[serde(default)]
    pub manager_scope: Option<TaskManagerScope>,
    #[serde(default)]
    pub task_session_id: Option<String>,
    #[serde(default)]
    pub required_for_parent_completion: Option<bool>,
    #[serde(default)]
    pub closure_state: Option<TaskClosureState>,
    #[serde(default)]
    pub closure_reason: Option<String>,
    #[serde(default)]
    pub superseded_by_run_id: Option<String>,
    #[serde(default)]
    pub superseded_by_task_id: Option<String>,
    #[serde(default)]
    pub repair_origin_verification_run_id: Option<String>,
    #[serde(default)]
    pub repair_attempt: u32,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub lifecycle_updated_at: Option<String>,
}
