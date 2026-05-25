use serde_json::{json, Value};

use crate::core::messages::select_preferred_text;
use crate::services::v2::ai_request_handler as v2_ai_request_handler;
use crate::services::v2::message_manager as v2_message_manager;
use crate::services::v3::ai_request_handler as v3_ai_request_handler;
use crate::services::v3::message_manager as v3_message_manager;

use super::super::actions_shared::normalize_inline_text;
use super::super::actions_vision_support::{BrowserVisionCandidate, BrowserVisionRunResult};
use super::super::DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserVisionTransport {
    Responses,
    ChatCompletions,
}

impl BrowserVisionTransport {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat_completions",
        }
    }
}

pub(crate) fn build_browser_vision_responses_input(prompt: &str, image_data_url: &str) -> Value {
    json!([
        {
            "type": "message",
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": prompt
                },
                {
                    "type": "input_image",
                    "image_url": image_data_url
                }
            ]
        }
    ])
}

pub(crate) fn build_browser_vision_chat_messages(
    prompt: &str,
    image_data_url: &str,
    system_prompt: Option<&str>,
    no_system_messages: bool,
) -> Vec<Value> {
    let wrapped_prompt =
        build_browser_vision_wrapped_prompt(prompt, system_prompt, no_system_messages);
    let user_content = json!([
        {
            "type": "text",
            "text": wrapped_prompt
        },
        {
            "type": "image_url",
            "image_url": {
                "url": image_data_url
            }
        }
    ]);

    if no_system_messages {
        return vec![json!({
            "role": "user",
            "content": user_content
        })];
    }

    let mut messages = Vec::new();
    if let Some(system_prompt) = system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));
    }
    messages.push(json!({
        "role": "user",
        "content": user_content
    }));
    messages
}

pub(super) async fn run_browser_vision_candidate(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
) -> Result<BrowserVisionRunResult, String> {
    let preferred_transport = preferred_browser_vision_transport(candidate);
    match run_browser_vision_candidate_once(prompt, image_data_url, candidate, preferred_transport)
        .await
    {
        Ok(analysis) => Ok(BrowserVisionRunResult {
            analysis,
            transport: preferred_transport.as_str(),
            transport_fallback_used: false,
        }),
        Err(primary_err) => {
            let Some(fallback_transport) = fallback_browser_vision_transport(preferred_transport)
            else {
                return Err(primary_err);
            };

            match run_browser_vision_candidate_once(
                prompt,
                image_data_url,
                candidate,
                fallback_transport,
            )
            .await
            {
                Ok(analysis) => Ok(BrowserVisionRunResult {
                    analysis,
                    transport: fallback_transport.as_str(),
                    transport_fallback_used: true,
                }),
                Err(fallback_err) => Err(format!(
                    "{} transport failed: {}; {} fallback failed: {}",
                    preferred_transport.as_str(),
                    normalize_inline_text(primary_err.as_str(), 220),
                    fallback_transport.as_str(),
                    normalize_inline_text(fallback_err.as_str(), 220)
                )),
            }
        }
    }
}

pub(crate) fn preferred_browser_vision_transport(
    candidate: &BrowserVisionCandidate,
) -> BrowserVisionTransport {
    if candidate.supports_responses {
        BrowserVisionTransport::Responses
    } else {
        BrowserVisionTransport::ChatCompletions
    }
}

fn build_browser_vision_wrapped_prompt(
    prompt: &str,
    system_prompt: Option<&str>,
    inline_system_context: bool,
) -> String {
    if !inline_system_context {
        return prompt.to_string();
    }

    let Some(system_prompt) = system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return prompt.to_string();
    };

    format!("【系统上下文】\n{}\n\n{}", system_prompt, prompt)
}

fn fallback_browser_vision_transport(
    transport: BrowserVisionTransport,
) -> Option<BrowserVisionTransport> {
    match transport {
        BrowserVisionTransport::Responses => Some(BrowserVisionTransport::ChatCompletions),
        BrowserVisionTransport::ChatCompletions => None,
    }
}

async fn run_browser_vision_candidate_once(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
    transport: BrowserVisionTransport,
) -> Result<String, String> {
    match transport {
        BrowserVisionTransport::Responses => {
            run_browser_vision_with_responses(prompt, image_data_url, candidate).await
        }
        BrowserVisionTransport::ChatCompletions => {
            run_browser_vision_with_chat_completions(prompt, image_data_url, candidate).await
        }
    }
}

async fn run_browser_vision_with_responses(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
) -> Result<String, String> {
    let handler = v3_ai_request_handler::AiRequestHandler::new(
        candidate.api_key.clone(),
        candidate.base_url.clone(),
        v3_message_manager::MessageManager::new(),
    );
    let no_system_messages =
        browser_vision_base_url_disallows_system_messages(candidate.base_url.as_str());
    let wrapped_prompt = build_browser_vision_wrapped_prompt(
        prompt,
        candidate.instructions.as_deref(),
        no_system_messages,
    );
    let input = build_browser_vision_responses_input(wrapped_prompt.as_str(), image_data_url);
    let response = handler
        .handle_request(
            input,
            candidate.model.clone(),
            if no_system_messages {
                None
            } else {
                candidate.instructions.clone()
            },
            None,
            None,
            None,
            Some(candidate.temperature),
            Some(DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS),
            v3_ai_request_handler::StreamCallbacks::default(),
            Some(candidate.provider.clone()),
            candidate.thinking_level.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
            browser_vision_request_purpose(candidate),
        )
        .await
        .map_err(|err| format!("responses transport request failed: {}", err))?;
    ensure_browser_vision_analysis(
        select_browser_vision_response_text(response.content, response.reasoning),
        "responses transport did not include text output",
    )
}

async fn run_browser_vision_with_chat_completions(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
) -> Result<String, String> {
    let handler = v2_ai_request_handler::AiRequestHandler::new(
        candidate.api_key.clone(),
        candidate.base_url.clone(),
        v2_message_manager::MessageManager::new(),
    );
    let no_system_messages =
        browser_vision_base_url_disallows_system_messages(candidate.base_url.as_str());
    let messages = build_browser_vision_chat_messages(
        prompt,
        image_data_url,
        candidate.instructions.as_deref(),
        no_system_messages,
    );
    let response = handler
        .handle_request(
            messages,
            None,
            candidate.model.clone(),
            Some(candidate.temperature),
            Some(DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS),
            v2_ai_request_handler::StreamCallbacks {
                on_chunk: None,
                on_thinking: None,
            },
            false,
            Some(candidate.provider.clone()),
            candidate.thinking_level.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
            browser_vision_request_purpose(candidate),
        )
        .await
        .map_err(|err| format!("chat/completions transport request failed: {}", err))?;
    ensure_browser_vision_analysis(
        select_browser_vision_response_text(response.content, response.reasoning),
        "chat/completions transport did not include text output",
    )
}

fn browser_vision_request_purpose(candidate: &BrowserVisionCandidate) -> &'static str {
    if candidate.prompt_source == "contact_agent" {
        "browser_vision_contact"
    } else {
        "browser_vision_fallback"
    }
}

fn browser_vision_base_url_disallows_system_messages(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    if let Ok(value) = std::env::var("DISABLE_SYSTEM_MESSAGES_FOR_PROXY") {
        let normalized = value.trim().to_lowercase();
        return normalized == "1"
            || normalized == "true"
            || normalized == "yes"
            || normalized == "on";
    }

    false
}

fn select_browser_vision_response_text(content: String, reasoning: Option<String>) -> String {
    select_preferred_text(content.as_str(), reasoning.as_deref())
        .map(str::trim)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn ensure_browser_vision_analysis(
    analysis: String,
    empty_output_error: &str,
) -> Result<String, String> {
    if analysis.trim().is_empty() {
        return Err(empty_output_error.to_string());
    }
    Ok(analysis)
}
