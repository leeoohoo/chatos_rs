use serde_json::{Value, json};
use uuid::Uuid;

use crate::core::ai_settings::chat_max_tokens_from_settings;
use crate::services::ai_common::normalize_turn_id;
use crate::utils::attachments::{self, Attachment};

use super::runtime_context::{
    ConversationRuntimeRequest, ResolvedConversationRuntimeContext, resolve_runtime_context,
};
use super::user_context::load_runtime_user_context;

pub struct CommonChatBootstrapInput {
    pub session_id: String,
    pub content: String,
    pub user_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub workspace_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub mcp_enabled: Option<bool>,
    pub enabled_mcp_ids: Option<Vec<String>>,
    pub auto_create_task: Option<bool>,
    pub skills_enabled: Option<bool>,
    pub selected_skill_ids: Option<Vec<String>>,
    pub turn_id: Option<String>,
    pub attachments: Option<Vec<Value>>,
    pub default_system_prompt: Option<String>,
    pub use_active_system_context: bool,
}

pub struct CommonChatBootstrap {
    pub effective_settings: Value,
    pub runtime_context: ResolvedConversationRuntimeContext,
    pub attachments: Vec<Attachment>,
    pub user_message_id: String,
    pub resolved_turn_id: String,
    pub max_tokens: Option<i64>,
}

pub async fn load_common_chat_bootstrap(input: CommonChatBootstrapInput) -> CommonChatBootstrap {
    let user_message_id = Uuid::new_v4().to_string();
    let resolved_turn_id =
        normalize_turn_id(input.turn_id.as_deref()).unwrap_or_else(|| user_message_id.clone());
    let user_context = load_runtime_user_context(input.user_id.clone(), &input.session_id).await;
    let effective_settings = if user_context.effective_settings.is_null() {
        json!({})
    } else {
        user_context.effective_settings.clone()
    };
    let runtime_context = resolve_runtime_context(
        &input.session_id,
        &input.content,
        &ConversationRuntimeRequest {
            effective_user_id: user_context.effective_user_id.clone(),
            contact_agent_id: input.contact_agent_id,
            project_id: input.project_id,
            project_root: input.project_root,
            workspace_root: input.workspace_root,
            remote_connection_id: input.remote_connection_id,
            mcp_enabled: input.mcp_enabled,
            enabled_mcp_ids: input.enabled_mcp_ids,
            auto_create_task: input.auto_create_task,
            skills_enabled: input.skills_enabled,
            selected_skill_ids: input.selected_skill_ids,
            conversation_turn_id: Some(resolved_turn_id.clone()),
            source_user_message_id: Some(user_message_id.clone()),
        },
        input.default_system_prompt,
        input.use_active_system_context,
        user_context.locale,
    )
    .await;
    let attachments = attachments::parse_attachments(&input.attachments.unwrap_or_default());
    let max_tokens = chat_max_tokens_from_settings(&effective_settings);

    CommonChatBootstrap {
        effective_settings,
        runtime_context,
        attachments,
        user_message_id,
        resolved_turn_id,
        max_tokens,
    }
}
