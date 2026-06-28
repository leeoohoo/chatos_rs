use chatos_ai_runtime as shared_ai_runtime;
use chatos_builtin_tools::BrowserVisionFailure;
use chatos_mcp_runtime::ToolCallerModelRuntime;
use serde_json::{json, Value};

use super::candidates::build_browser_vision_candidates;
use super::context::prepare_browser_vision_context;
use super::support::{
    build_browser_vision_image_data_url, build_browser_vision_prompt,
    build_browser_vision_unavailable_message, normalize_inline_text,
};
use super::types::{
    BrowserVisionCandidate, BrowserVisionOutput, BrowserVisionRunResult, BROWSER_VISION_TRANSPORT,
    DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS,
};

pub(super) async fn analyze_screenshot_with_best_available_runtime(
    question: &str,
    screenshot_path: &str,
    conversation_id: Option<&str>,
    caller_model_runtime: Option<&ToolCallerModelRuntime>,
) -> Result<BrowserVisionOutput, BrowserVisionFailure> {
    let prepared = prepare_browser_vision_context(conversation_id).await;
    let mut warnings = prepared.warnings.clone();
    let candidates =
        build_browser_vision_candidates(&prepared, caller_model_runtime, &mut warnings).await;
    if candidates.is_empty() {
        return Err(BrowserVisionFailure {
            error: build_browser_vision_unavailable_message(warnings.as_slice()),
            attempts: Vec::new(),
            warnings,
        });
    }

    let image_data_url = build_browser_vision_image_data_url(screenshot_path)
        .await
        .map_err(|err| BrowserVisionFailure {
            error: err,
            attempts: Vec::new(),
            warnings: warnings.clone(),
        })?;
    let prompt = build_browser_vision_prompt(question);
    let total_candidates = candidates.len();
    let mut attempts = Vec::new();
    let mut last_error = String::new();

    for (index, candidate) in candidates.into_iter().enumerate() {
        match run_browser_vision_candidate(prompt.as_str(), image_data_url.as_str(), &candidate)
            .await
        {
            Ok(run_result) => {
                let attempt_provider = candidate.provider.clone();
                let attempt_model = candidate.model.clone();
                attempts.push(json!({
                    "mode": candidate.mode,
                    "prompt_source": candidate.prompt_source,
                    "provider": attempt_provider,
                    "model": attempt_model,
                    "transport": run_result.transport,
                    "status": "success"
                }));
                return Ok(BrowserVisionOutput {
                    analysis: run_result.analysis,
                    mode: candidate.mode.to_string(),
                    prompt_source: candidate.prompt_source.to_string(),
                    contact_agent_id: candidate.contact_agent_id.clone(),
                    model: candidate.model,
                    provider: candidate.provider,
                    transport: run_result.transport.to_string(),
                    fallback_used: index > 0,
                    attempts,
                    warnings,
                });
            }
            Err(err) => {
                last_error = err.clone();
                let attempt_provider = candidate.provider.clone();
                let attempt_model = candidate.model.clone();
                attempts.push(json!({
                    "mode": candidate.mode,
                    "prompt_source": candidate.prompt_source,
                    "provider": attempt_provider,
                    "model": attempt_model,
                    "transport": BROWSER_VISION_TRANSPORT,
                    "status": "error",
                    "error": normalize_inline_text(err.as_str(), 220)
                }));
            }
        }
    }

    Err(BrowserVisionFailure {
        error: format!(
            "vision analysis failed for all {} candidate(s). Last error: {}",
            total_candidates,
            normalize_inline_text(last_error.as_str(), 220)
        ),
        attempts,
        warnings,
    })
}

fn build_browser_vision_responses_input(prompt: &str, image_data_url: &str) -> Value {
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

async fn run_browser_vision_candidate(
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

async fn run_browser_vision_with_responses(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
) -> Result<String, String> {
    let handler = shared_ai_runtime::AiRequestHandler::new();
    let runtime = shared_ai_runtime::ModelRuntimeConfig::openai_compatible(
        candidate.base_url.clone(),
        candidate.api_key.clone(),
        candidate.model.clone(),
        candidate.provider.clone(),
    )
    .with_responses_support(true)
    .with_instructions(candidate.instructions.clone())
    .with_temperature(Some(candidate.temperature))
    .with_max_output_tokens(Some(DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS))
    .with_thinking_level(candidate.thinking_level.clone())
    .with_request_body_limit_bytes(candidate.request_body_limit_bytes);
    let response = shared_ai_runtime::run_compatible_prompt_with(
        &handler,
        &runtime,
        prompt,
        shared_ai_runtime::SimplePromptOptions {
            system_prompt: candidate.instructions.clone(),
            temperature: Some(candidate.temperature),
            max_output_tokens: Some(DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS),
            max_attempts: Some(4),
            callbacks: shared_ai_runtime::StreamCallbacks::default(),
        },
        |wrapped_prompt, _input_as_list| {
            build_browser_vision_responses_input(wrapped_prompt, image_data_url)
        },
    );
    let response = response
        .await
        .map_err(|err| format!("responses transport request failed: {}", err))?;
    ensure_browser_vision_analysis(
        shared_ai_runtime::select_preferred_response_text(
            response.content.as_str(),
            response.reasoning.as_deref(),
        )
        .map(str::trim)
        .map(|value| value.to_string())
        .unwrap_or_default(),
        "responses transport did not include text output",
    )
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
