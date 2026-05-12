use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::Value;

use crate::core::mcp_runtime::McpServerBundle;
use crate::core::mcp_tools::ToolInfo;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::memory_runtime_types::TurnRuntimeSnapshotSelectedCommandDto;

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
    pub turn_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub mcp_enabled: Option<bool>,
    pub enabled_mcp_ids: Option<Vec<String>>,
    pub skills_enabled: Option<bool>,
    pub selected_skill_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedChatStreamContext {
    #[allow(dead_code)]
    pub effective_user_id: Option<String>,
    pub internal_context_locale: InternalContextLocale,
    pub contact_agent_id: Option<String>,
    pub base_system_prompt: Option<String>,
    pub contact_system_prompt: Option<String>,
    pub builtin_mcp_system_prompt: Option<String>,
    pub command_system_prompt: Option<String>,
    pub selected_commands_for_snapshot: Arc<Mutex<Vec<TurnRuntimeSnapshotSelectedCommandDto>>>,
    pub resolved_project_id: Option<String>,
    pub resolved_project_root: Option<String>,
    pub default_remote_connection_id: Option<String>,
    pub workspace_root: Option<String>,
    pub mcp_enabled: bool,
    pub enabled_mcp_ids_for_snapshot: Vec<String>,
    pub mcp_server_bundle: McpServerBundle,
    pub use_tools: bool,
    pub memory_summary_prompt: Option<String>,
}

pub(crate) type ToolMetadataMap = HashMap<String, ToolInfo>;
