// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp_runtime::ToolCallerModelRuntime;
use serde_json::Value;

use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::repositories::ai_model_configs;
use crate::utils::attachments::is_vision_model;

use super::support::{ai_model_config_to_runtime_value, normalize_non_empty};
use super::types::{BrowserVisionCandidate, BrowserVisionPreparedContext};

pub(super) async fn build_browser_vision_candidates(
    prepared: &BrowserVisionPreparedContext,
    caller_model_runtime: Option<&ToolCallerModelRuntime>,
    warnings: &mut Vec<String>,
) -> Vec<BrowserVisionCandidate> {
    let prompt_source = browser_vision_prompt_source(prepared);
    let instructions = prepared.contact_system_prompt.clone();
    let contact_agent_id = prepared.contact_agent_id.clone();
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    append_caller_model_candidate(
        caller_model_runtime,
        warnings,
        prompt_source,
        contact_agent_id.clone(),
        instructions.clone(),
        &mut out,
        &mut seen,
    );
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

fn browser_vision_candidate_from_model_cfg(
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
    if !runtime.supports_responses {
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
        request_body_limit_bytes: None,
    })
}

pub(super) fn browser_vision_candidate_from_caller_runtime(
    runtime: &ToolCallerModelRuntime,
    prompt_source: &'static str,
    contact_agent_id: Option<String>,
    instructions: Option<String>,
) -> Option<BrowserVisionCandidate> {
    let model = normalize_non_empty(Some(runtime.model.as_str()))?;
    let base_url = normalize_non_empty(Some(runtime.base_url.as_str()))?;
    let api_key = normalize_non_empty(Some(runtime.api_key.as_str()))?;
    let provider =
        normalize_non_empty(Some(runtime.provider.as_str())).unwrap_or_else(|| "gpt".to_string());
    let supports_images = runtime
        .supports_images
        .unwrap_or_else(|| is_vision_model(model.as_str()));
    if !supports_images {
        return None;
    }
    if !runtime.supports_responses {
        return None;
    }

    Some(BrowserVisionCandidate {
        mode: "caller_model",
        prompt_source,
        contact_agent_id,
        instructions,
        model,
        provider,
        thinking_level: runtime.thinking_level.clone(),
        temperature: runtime.temperature.unwrap_or(0.7),
        api_key,
        base_url,
        request_body_limit_bytes: runtime.request_body_limit_bytes,
    })
}

fn default_browser_vision_candidate(
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
        request_body_limit_bytes: None,
    })
}

fn push_browser_vision_candidate(
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

fn model_cfg_supports_browser_vision(model_cfg: &Value, resolved_model: &str) -> bool {
    model_cfg
        .get("supports_images")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || is_vision_model(resolved_model)
}

fn append_caller_model_candidate(
    caller_model_runtime: Option<&ToolCallerModelRuntime>,
    warnings: &mut Vec<String>,
    prompt_source: &'static str,
    contact_agent_id: Option<String>,
    instructions: Option<String>,
    out: &mut Vec<BrowserVisionCandidate>,
    seen: &mut HashSet<String>,
) {
    let Some(runtime) = caller_model_runtime else {
        return;
    };
    if !runtime.is_configured() {
        warnings.push(
            "Current caller model runtime is incomplete, so browser_vision will try fallback models."
                .to_string(),
        );
        return;
    }
    if let Some(candidate) = browser_vision_candidate_from_caller_runtime(
        runtime,
        prompt_source,
        contact_agent_id,
        instructions,
    ) {
        push_browser_vision_candidate(out, seen, candidate);
    } else {
        warnings.push(
            "Current caller model is not available for browser_vision, so fallback models will be tried."
                .to_string(),
        );
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
