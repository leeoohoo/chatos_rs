// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChatStreamRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    pub conversation_id: Option<String>,
    pub content: Option<String>,
    pub model_config_id: Option<String>,
    pub ai_model_config: Option<Value>,
    pub user_id: Option<String>,
    #[serde(skip_deserializing)]
    pub user_role: Option<String>,
    pub attachments: Option<Vec<Value>>,
    pub reasoning_enabled: Option<bool>,
    pub turn_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    #[serde(alias = "workspaceRoot")]
    pub workspace_root: Option<String>,
    pub remote_connection_id: Option<String>,
    #[serde(default, alias = "taskPluginPreferences")]
    pub task_plugin_preferences: Vec<String>,
    #[serde(
        default,
        rename = "plugin_agent_selection",
        alias = "pluginAgentSelection"
    )]
    pub unsupported_plugin_agent_selection: Option<Value>,
    #[serde(skip_deserializing)]
    pub user_message_id: Option<String>,
}

#[cfg(test)]
mod planning_mode_removal_tests {
    use super::*;

    #[test]
    fn rejects_removed_planning_fields_without_changing_reasoning() {
        for key in ["plan_mode", "planMode", "plan_mode_enabled"] {
            let request = serde_json::json!({"content": "hello", key: true});
            assert!(serde_json::from_value::<ChatStreamRequest>(request).is_err());
        }
        let request: ChatStreamRequest = serde_json::from_value(serde_json::json!({
            "content": "hello", "reasoning_enabled": true
        }))
        .unwrap();
        assert_eq!(request.reasoning_enabled, Some(true));
    }
}
