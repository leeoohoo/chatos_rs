// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct SessionQuery {
    pub(super) user_id: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) limit: Option<String>,
    pub(super) offset: Option<String>,
    pub(super) include_archived: Option<String>,
    pub(super) include_archiving: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateSessionRequest {
    pub(super) title: Option<String>,
    pub(super) description: Option<String>,
    pub(super) metadata: Option<Value>,
    pub(super) user_id: Option<String>,
    pub(super) project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateSessionRequest {
    pub(super) title: Option<String>,
    pub(super) description: Option<String>,
    pub(super) metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateMessageRequest {
    pub(super) role: Option<String>,
    pub(super) content: Option<String>,
    #[serde(alias = "messageMode")]
    pub(super) message_mode: Option<String>,
    #[serde(alias = "messageSource")]
    pub(super) message_source: Option<String>,
    pub(super) tool_calls: Option<Value>,
    pub(super) tool_call_id: Option<String>,
    pub(super) reasoning: Option<String>,
    pub(super) metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PageQuery {
    pub(super) limit: Option<String>,
    pub(super) offset: Option<String>,
    pub(super) compact: Option<String>,
    pub(super) strategy: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompactHistoryQuery {
    pub(super) limit: Option<String>,
    pub(super) before: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AddMcpServerRequest {
    pub(super) mcp_server_name: Option<String>,
    pub(super) mcp_config_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateSessionRuntimeSettingsRequest {
    #[serde(alias = "selectedModelId")]
    pub(super) selected_model_id: Option<Option<String>>,
    #[serde(alias = "selectedModelName")]
    pub(super) selected_model_name: Option<Option<String>>,
    #[serde(alias = "selectedThinkingLevel")]
    pub(super) selected_thinking_level: Option<Option<String>>,
    #[serde(alias = "remoteConnectionId")]
    pub(super) remote_connection_id: Option<Option<String>>,
    #[serde(alias = "workspaceRoot")]
    pub(super) workspace_root: Option<Option<String>>,
    #[serde(alias = "reasoningEnabled")]
    pub(super) reasoning_enabled: Option<bool>,
    #[serde(alias = "planModeEnabled")]
    pub(super) plan_mode_enabled: Option<bool>,
    #[serde(alias = "mcpEnabled")]
    pub(super) mcp_enabled: Option<bool>,
    #[serde(alias = "enabledMcpIds")]
    pub(super) enabled_mcp_ids: Option<Vec<String>>,
    #[serde(alias = "autoCreateTask")]
    pub(super) auto_create_task: Option<bool>,
}
