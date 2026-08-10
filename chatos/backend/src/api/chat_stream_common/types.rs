// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::PluginCommandInvocation;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct ChatStreamRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    pub conversation_id: Option<String>,
    pub content: Option<String>,
    pub model_config_id: Option<String>,
    pub ai_model_config: Option<Value>,
    pub user_id: Option<String>,
    pub attachments: Option<Vec<Value>>,
    pub reasoning_enabled: Option<bool>,
    #[serde(default, alias = "planMode")]
    pub plan_mode: bool,
    pub turn_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    #[serde(alias = "workspaceRoot")]
    pub workspace_root: Option<String>,
    pub remote_connection_id: Option<String>,
    #[serde(default, alias = "selectedPluginIds")]
    pub selected_plugin_ids: Vec<String>,
    #[serde(default, alias = "pluginCommandInvocations")]
    pub plugin_command_invocations: Vec<PluginCommandInvocation>,
    #[serde(
        default,
        rename = "plugin_agent_selection",
        alias = "pluginAgentSelection"
    )]
    pub unsupported_plugin_agent_selection: Option<Value>,
    #[serde(skip_deserializing)]
    pub user_message_id: Option<String>,
    #[serde(skip_deserializing)]
    pub project_requirement_execution_planner: bool,
    #[serde(skip_deserializing, default)]
    pub project_requirement_execution_task_ids: Vec<String>,
}
