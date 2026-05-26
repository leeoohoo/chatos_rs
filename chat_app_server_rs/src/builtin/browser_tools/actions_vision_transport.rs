use serde_json::{json, Value};

use crate::core::messages::select_preferred_text;
use crate::services::v3::ai_request_handler as v3_ai_request_handler;
use crate::services::v3::message_manager as v3_message_manager;

use super::super::actions_vision_support::{BrowserVisionCandidate, BrowserVisionRunResult};
use super::super::DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS;

pub(crate) const BROWSER_VISION_TRANSPORT: &str = "responses";

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

pub(super) async fn run_browser_vision_candidate(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
) -> Result<BrowserVisionRunResult, String> {
    let analysis = run_browser_vision_with_responses(prompt, image_data_url, candidate).await?;
    Ok(BrowserVisionRunResult {
        analysis,
        transport: BROWSER_VISION_TRANSPORT,
    })
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
