use std::collections::HashSet;

use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::core::chat_runtime::{
    compose_contact_system_prompt, ChatRuntimeMetadata, ContactSkillPromptMode,
};
use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::{ai_model_config::AiModelConfig, session::Session};
use crate::repositories::ai_model_configs;
use crate::services::{chatos_agents, chatos_sessions};
use crate::utils::attachments::is_vision_model;

use super::actions_shared::normalize_inline_text;

#[derive(Debug, Clone)]
pub(super) struct BrowserVisionPreparedContext {
    pub(super) session_model_cfg: Option<Value>,
    pub(super) selected_model_id: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) contact_agent_id: Option<String>,
    pub(super) contact_system_prompt: Option<String>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct BrowserVisionCandidate {
    pub(super) mode: &'static str,
    pub(super) prompt_source: &'static str,
    pub(super) contact_agent_id: Option<String>,
    pub(super) instructions: Option<String>,
    pub(super) model: String,
    pub(super) provider: String,
    pub(super) thinking_level: Option<String>,
    pub(super) temperature: f64,
    pub(super) api_key: String,
    pub(super) base_url: String,
}

#[derive(Debug, Clone)]
pub(super) struct BrowserVisionRunResult {
    pub(super) analysis: String,
    pub(super) transport: &'static str,
}

pub(super) fn build_browser_vision_prompt(question: &str) -> String {
    format!(
        "你现在收到了一张当前网页截图。请仅基于截图内容回答用户问题，先给结论，再给1-3条关键依据。用户问题：{}",
        question
    )
}

pub(super) fn build_browser_vision_unavailable_message(warnings: &[String]) -> String {
    if warnings.is_empty() {
        "browser_vision has no available vision-capable model configuration.".to_string()
    } else {
        format!(
            "browser_vision has no available vision-capable model configuration. {}",
            warnings
                .iter()
                .map(|item| normalize_inline_text(item.as_str(), 180))
                .collect::<Vec<_>>()
                .join(" | ")
        )
    }
}

pub(super) fn browser_vision_candidate_from_model_cfg(
    model_cfg: &Value,
    mode: &'static str,
    prompt_source: &'static str,
    contact_agent_id: Option<String>,
    instructions: Option<String>,
) -> Option<BrowserVisionCandidate> {
    let cfg = Config::try_get().ok()?;
    let runtime = resolve_chat_model_config(
        model_cfg,
        "gpt-4o",
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        Some(true),
        true,
    );
    if runtime.api_key.trim().is_empty() || runtime.base_url.trim().is_empty() {
        return None;
    }
    if !model_cfg_supports_browser_vision(model_cfg, runtime.model.as_str()) {
        return None;
    }

    Some(BrowserVisionCandidate {
        mode,
        prompt_source,
        contact_agent_id,
        instructions,
        model: runtime.model,
        provider: runtime.provider,
        thinking_level: runtime.thinking_level,
        temperature: runtime.temperature,
        api_key: runtime.api_key,
        base_url: runtime.base_url,
    })
}

pub(super) fn default_browser_vision_candidate(
    prompt_source: &'static str,
    contact_agent_id: Option<String>,
    instructions: Option<String>,
) -> Option<BrowserVisionCandidate> {
    let cfg = Config::try_get().ok()?;
    if cfg.openai_api_key.trim().is_empty() || cfg.openai_base_url.trim().is_empty() {
        return None;
    }

    Some(BrowserVisionCandidate {
        mode: "default_model",
        prompt_source,
        contact_agent_id,
        instructions,
        model: "gpt-4o".to_string(),
        provider: "gpt".to_string(),
        thinking_level: None,
        temperature: 0.7,
        api_key: cfg.openai_api_key.clone(),
        base_url: cfg.openai_base_url.clone(),
    })
}

pub(super) fn push_browser_vision_candidate(
    out: &mut Vec<BrowserVisionCandidate>,
    seen: &mut HashSet<String>,
    candidate: BrowserVisionCandidate,
) {
    let signature = format!(
        "{}|{}|{}",
        candidate.provider, candidate.model, candidate.base_url
    );
    if seen.insert(signature) {
        out.push(candidate);
    }
}

pub(super) fn model_cfg_supports_browser_vision(model_cfg: &Value, resolved_model: &str) -> bool {
    model_cfg
        .get("supports_images")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || is_vision_model(resolved_model)
}

pub(super) fn json_value_is_empty_object(value: &Value) -> bool {
    value
        .as_object()
        .map(|items| items.is_empty())
        .unwrap_or(false)
}

pub(super) fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

pub(super) fn ai_model_config_to_runtime_value(model_cfg: &AiModelConfig) -> Value {
    json!({
        "id": model_cfg.id,
        "name": model_cfg.name,
        "provider": model_cfg.provider,
        "model_name": model_cfg.model,
        "thinking_level": model_cfg.thinking_level,
        "api_key": model_cfg.api_key,
        "base_url": model_cfg.base_url,
        "enabled": model_cfg.enabled,
        "supports_images": model_cfg.supports_images,
        "supports_reasoning": model_cfg.supports_reasoning,
        "supports_responses": model_cfg.supports_responses,
    })
}

pub(super) async fn build_browser_vision_image_data_url(
    screenshot_path: &str,
) -> Result<String, String> {
    let image_bytes = tokio::fs::read(screenshot_path)
        .await
        .map_err(|err| format!("read screenshot failed: {}", err))?;
    let mime = mime_guess::from_path(screenshot_path).first_or_octet_stream();
    Ok(format!(
        "data:{};base64,{}",
        mime.essence_str(),
        BASE64_STD.encode(image_bytes)
    ))
}

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

pub(super) async fn build_browser_vision_candidates(
    prepared: &BrowserVisionPreparedContext,
    warnings: &mut Vec<String>,
) -> Vec<BrowserVisionCandidate> {
    let prompt_source = browser_vision_prompt_source(prepared);
    let instructions = prepared.contact_system_prompt.clone();
    let contact_agent_id = prepared.contact_agent_id.clone();
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    append_session_model_candidate(
        prepared,
        warnings,
        prompt_source,
        contact_agent_id.clone(),
        instructions.clone(),
        &mut out,
        &mut seen,
    );
    append_user_model_candidates(
        prepared,
        warnings,
        prompt_source,
        contact_agent_id.clone(),
        instructions.clone(),
        &mut out,
        &mut seen,
    )
    .await;

    if let Some(candidate) =
        default_browser_vision_candidate(prompt_source, contact_agent_id, instructions)
    {
        push_browser_vision_candidate(&mut out, &mut seen, candidate);
    } else {
        warnings.push(
            "No global OPENAI_API_KEY fallback is configured for browser_vision.".to_string(),
        );
    }

    out
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

fn browser_vision_prompt_source(prepared: &BrowserVisionPreparedContext) -> &'static str {
    if prepared.contact_system_prompt.is_some() {
        "contact_agent"
    } else {
        "generic"
    }
}

fn append_session_model_candidate(
    prepared: &BrowserVisionPreparedContext,
    warnings: &mut Vec<String>,
    prompt_source: &'static str,
    contact_agent_id: Option<String>,
    instructions: Option<String>,
    out: &mut Vec<BrowserVisionCandidate>,
    seen: &mut HashSet<String>,
) {
    let Some(model_cfg) = prepared.session_model_cfg.as_ref() else {
        return;
    };

    if let Some(candidate) = browser_vision_candidate_from_model_cfg(
        model_cfg,
        "session_model",
        prompt_source,
        contact_agent_id,
        instructions,
    ) {
        push_browser_vision_candidate(out, seen, candidate);
    } else {
        warnings.push(
            "Current session model is unavailable for browser_vision, so a fallback model will be used."
                .to_string(),
        );
    }
}

async fn append_user_model_candidates(
    prepared: &BrowserVisionPreparedContext,
    warnings: &mut Vec<String>,
    prompt_source: &'static str,
    contact_agent_id: Option<String>,
    instructions: Option<String>,
    out: &mut Vec<BrowserVisionCandidate>,
    seen: &mut HashSet<String>,
) {
    let Some(_user_id) = prepared.user_id.as_deref() else {
        return;
    };

    match ai_model_configs::list_ai_model_configs(prepared.user_id.as_deref()).await {
        Ok(configs) => {
            for model_cfg in configs.into_iter().filter(|cfg| cfg.enabled) {
                if prepared.selected_model_id.as_deref() == Some(model_cfg.id.as_str()) {
                    continue;
                }
                let value = ai_model_config_to_runtime_value(&model_cfg);
                if let Some(candidate) = browser_vision_candidate_from_model_cfg(
                    &value,
                    "user_model",
                    prompt_source,
                    contact_agent_id.clone(),
                    instructions.clone(),
                ) {
                    push_browser_vision_candidate(out, seen, candidate);
                }
            }
        }
        Err(err) => warnings.push(format!(
            "list enabled image-capable model configs failed: {}",
            err
        )),
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
