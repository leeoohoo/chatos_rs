// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::core::chat_runtime::{
    compose_contact_system_prompt, ChatRuntimeMetadata, ContactSkillPromptMode,
};
use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::session::Session;
use crate::repositories::ai_model_configs;
use crate::services::{chatos_agents, chatos_sessions};

use super::support::{
    ai_model_config_to_runtime_value, json_value_is_empty_object, normalize_non_empty,
};
use super::types::BrowserVisionPreparedContext;

pub(super) async fn prepare_browser_vision_context(
    conversation_id: Option<&str>,
) -> BrowserVisionPreparedContext {
    let mut context = BrowserVisionPreparedContext {
        session_model_cfg: None,
        selected_model_id: None,
        user_id: None,
        contact_agent_id: None,
        contact_system_prompt: None,
        warnings: Vec::new(),
    };

    let Some(conversation_id) = normalize_non_empty(conversation_id) else {
        context.warnings.push(
            "No active conversation_id was available, so browser_vision will skip session/contact context."
                .to_string(),
        );
        return context;
    };

    let Some(session) =
        load_browser_vision_session(conversation_id.as_str(), &mut context.warnings).await
    else {
        return context;
    };

    context.user_id = normalize_non_empty(session.user_id.as_deref());
    context.selected_model_id = normalize_non_empty(session.selected_model_id.as_deref());

    populate_session_model_cfg(&session, &mut context).await;
    populate_contact_prompt(&session, &mut context).await;
    context
}

async fn load_browser_vision_session(
    conversation_id: &str,
    warnings: &mut Vec<String>,
) -> Option<Session> {
    match chatos_sessions::get_session_by_id(conversation_id).await {
        Ok(Some(session)) => Some(session),
        Ok(None) => {
            warnings.push(format!("conversation not found: {}", conversation_id));
            None
        }
        Err(err) => {
            warnings.push(format!("load current session failed: {}", err));
            None
        }
    }
}

async fn populate_session_model_cfg(session: &Session, context: &mut BrowserVisionPreparedContext) {
    if context.selected_model_id.is_none() {
        return;
    }

    match load_session_model_cfg_value(session).await {
        Ok(value) if !json_value_is_empty_object(&value) => {
            context.session_model_cfg = Some(value);
        }
        Ok(_) => context.warnings.push(
            "Current session has a selected model id, but the model config could not be loaded."
                .to_string(),
        ),
        Err(err) => context
            .warnings
            .push(format!("load current session model config failed: {}", err)),
    }
}

async fn populate_contact_prompt(session: &Session, context: &mut BrowserVisionPreparedContext) {
    let metadata_runtime = ChatRuntimeMetadata::from_metadata(session.metadata.as_ref());
    context.contact_agent_id = normalize_non_empty(session.selected_agent_id.as_deref())
        .or_else(|| metadata_runtime.contact_agent_id.clone());

    let Some(contact_agent_id) = context.contact_agent_id.clone() else {
        context.warnings.push(
            "Current session has no selected contact agent, so browser_vision will use a generic prompt."
                .to_string(),
        );
        return;
    };

    match chatos_agents::get_agent_runtime_context(contact_agent_id.as_str()).await {
        Ok(Some(runtime)) => {
            context.contact_system_prompt = normalize_non_empty(
                compose_contact_system_prompt(
                    Some(&runtime),
                    &ContactSkillPromptMode::Disabled,
                    InternalContextLocale::ZhCn,
                )
                .as_deref(),
            );
        }
        Ok(None) => context.warnings.push(format!(
            "contact runtime context not found for agent {}",
            contact_agent_id
        )),
        Err(err) => context
            .warnings
            .push(format!("load contact runtime context failed: {}", err)),
    }
}

async fn load_session_model_cfg_value(session: &Session) -> Result<Value, String> {
    let Some(model_id) = normalize_non_empty(session.selected_model_id.as_deref()) else {
        return Ok(json!({}));
    };
    let Some(model_cfg) = ai_model_configs::get_ai_model_config_by_id(model_id.as_str()).await?
    else {
        return Ok(json!({}));
    };
    if model_cfg.user_id.as_deref() != session.user_id.as_deref() {
        return Ok(json!({}));
    }
    Ok(ai_model_config_to_runtime_value(&model_cfg))
}
