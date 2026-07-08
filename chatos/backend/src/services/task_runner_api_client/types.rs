// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use crate::services::builtin_mcp::BuiltinMcpKind;

#[derive(Debug, Clone)]
pub struct UserServiceTaskRunnerExchange {
    pub base_url: String,
    pub access_token: String,
    pub task_runner_agent_account_id: String,
    pub contact_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UserServiceTaskRunnerTokenResponse {
    pub(super) access_token: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskRunnerSkillResponse {
    pub(super) content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskRunnerTaskRecord {
    pub id: String,
    pub status: String,
    pub last_run_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TaskRunnerMcpConfigRequest {
    pub enabled_builtin_kinds: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin_prompt_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_manager_base_url: Option<String>,
    pub external_mcp_config_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ephemeral_http_servers: Vec<TaskRunnerEphemeralHttpMcpServerRequest>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TaskRunnerEphemeralHttpMcpServerRequest {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateTaskRunnerTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub objective: String,
    pub input_payload: Option<Value>,
    pub status: Option<String>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub default_model_config_id: Option<String>,
    pub project_id: Option<String>,
    pub task_profile: Option<String>,
    pub schedule: Option<TaskRunnerTaskScheduleRequest>,
    pub mcp_config: Option<TaskRunnerMcpConfigRequest>,
    pub prerequisite_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TaskRunnerTaskScheduleRequest {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CancelTaskRunnerTaskRequest {
    pub reason: String,
    pub replacement_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct TaskRunnerMcpCatalogEntry {
    pub(super) kind: String,
    pub(super) config_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct TaskRunnerExternalMcpConfig {
    pub(super) id: String,
    pub(super) enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct TaskRunnerSkillListItem {
    pub(super) id: String,
    #[serde(default)]
    pub(super) enabled: bool,
    #[serde(default)]
    pub(super) install_status: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskRunnerExecutionOptions {
    pub(super) builtin_tool_ids: BTreeSet<String>,
    pub(super) external_tool_ids: BTreeSet<String>,
    pub(super) skill_ids: BTreeSet<String>,
}

impl TaskRunnerExecutionOptions {
    pub fn mcp_config_for_tool_ids(
        &self,
        values: &[String],
    ) -> Result<TaskRunnerMcpConfigRequest, String> {
        let values = normalize_tool_ids(values);
        if values.is_empty() {
            return Err("task_runner_enabled_tool_ids is required".to_string());
        }
        let mut enabled_builtin_kinds = Vec::new();
        let mut external_mcp_config_ids = Vec::new();
        for value in values {
            if self.builtin_tool_ids.contains(value.as_str()) {
                enabled_builtin_kinds.push(value);
            } else if self.external_tool_ids.contains(value.as_str()) {
                external_mcp_config_ids.push(value);
            } else {
                return Err(format!("Task Runner 工具不可用或无权限访问: {value}"));
            }
        }
        Ok(TaskRunnerMcpConfigRequest {
            enabled_builtin_kinds,
            builtin_prompt_locale: None,
            workspace_dir: None,
            sandbox_enabled: None,
            sandbox_manager_base_url: None,
            external_mcp_config_ids,
            ephemeral_http_servers: Vec::new(),
            skill_ids: Vec::new(),
        })
    }

    pub fn validate_skill_ids(&self, values: Vec<String>) -> Result<Vec<String>, String> {
        let values = normalize_id_list(values);
        for value in &values {
            if !self.skill_ids.contains(value.as_str()) {
                return Err(format!("Task Runner Skill 不可用或无权限访问: {value}"));
            }
        }
        Ok(values)
    }
}

#[derive(Debug, Default, Serialize)]
pub struct SubmitTaskRunnerPromptRequest {
    pub values: Option<Value>,
    pub selection: Option<Value>,
    pub reason: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct CancelTaskRunnerPromptRequest {
    pub reason: Option<String>,
}

pub(super) fn normalize_tool_ids(values: &[String]) -> Vec<String> {
    normalize_id_list_with_aliases(values.to_vec())
}

fn normalize_id_list_with_aliases(values: Vec<String>) -> Vec<String> {
    let mut out = values
        .into_iter()
        .filter_map(normalize_tool_id_with_legacy_alias)
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut out = values
        .into_iter()
        .filter_map(|value| normalize_optional(Some(value)))
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_tool_id_with_legacy_alias(value: String) -> Option<String> {
    let value = normalize_optional(Some(value))?;
    Some(
        legacy_task_runner_tool_alias(value.as_str())
            .unwrap_or(value.as_str())
            .to_string(),
    )
}

fn legacy_task_runner_tool_alias(value: &str) -> Option<&'static str> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.starts_with("sandbox_terminal_") || normalized == "sandbox_terminal" {
        return Some(BuiltinMcpKind::TerminalController.kind_name());
    }
    if normalized == "sandbox_filesystem"
        || normalized == "sandbox_filesystem_rw"
        || normalized == "sandbox_code_maintainer"
    {
        return Some(BuiltinMcpKind::CodeMaintainerWrite.kind_name());
    }
    if !normalized.starts_with("sandbox_filesystem_") {
        return None;
    }
    if legacy_sandbox_filesystem_alias_is_write(normalized.as_str()) {
        Some(BuiltinMcpKind::CodeMaintainerWrite.kind_name())
    } else {
        Some(BuiltinMcpKind::CodeMaintainerRead.kind_name())
    }
}

fn legacy_sandbox_filesystem_alias_is_write(value: &str) -> bool {
    [
        "write", "edit", "append", "delete", "remove", "patch", "create", "rename", "move",
        "mkdir", "rmdir",
    ]
    .iter()
    .any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_legacy_sandbox_filesystem_aliases() {
        let values = normalize_tool_ids(&[
            "sandbox_filesystem_read_file_raw".to_string(),
            "sandbox_filesystem_search_text".to_string(),
            "sandbox_filesystem_write_file".to_string(),
            "sandbox_filesystem_apply_patch".to_string(),
        ]);

        assert_eq!(
            values,
            vec![
                BuiltinMcpKind::CodeMaintainerRead.kind_name().to_string(),
                BuiltinMcpKind::CodeMaintainerWrite.kind_name().to_string(),
            ]
        );
    }

    #[test]
    fn normalizes_legacy_sandbox_terminal_aliases() {
        let values = normalize_tool_ids(&[
            "sandbox_terminal_execute_command".to_string(),
            "sandbox_terminal_process_poll".to_string(),
        ]);

        assert_eq!(
            values,
            vec![BuiltinMcpKind::TerminalController.kind_name().to_string()]
        );
    }
}
