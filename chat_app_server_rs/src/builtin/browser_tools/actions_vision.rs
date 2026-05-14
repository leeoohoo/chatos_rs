#[path = "actions_vision_transport.rs"]
mod transport;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::builtin::browser_tools::context;

use self::transport::run_browser_vision_candidate;
#[allow(unused_imports)]
pub(super) use self::transport::{
    build_browser_vision_chat_messages, build_browser_vision_responses_input,
    preferred_browser_vision_transport, BrowserVisionTransport,
};
use super::actions_shared::{fail_json, is_success, normalize_inline_text, run_browser_command};
#[allow(unused_imports)]
pub(super) use super::actions_vision_support::ai_model_config_to_runtime_value;
pub(super) use super::actions_vision_support::build_browser_vision_unavailable_message;
use super::actions_vision_support::{
    build_browser_vision_candidates, build_browser_vision_image_data_url,
    build_browser_vision_prompt, prepare_browser_vision_context,
};
use super::BoundContext;

pub(super) async fn browser_vision_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    question: String,
    annotate: bool,
) -> Result<Value, String> {
    let session = context::conversation_key(conversation_id);
    let screenshot_dir = ctx
        .workspace_dir
        .join(".chatos")
        .join("browser_screenshots");
    std::fs::create_dir_all(&screenshot_dir)
        .map_err(|err| format!("create screenshot dir failed: {}", err))?;
    let screenshot_path = screenshot_dir.join(format!(
        "browser_screenshot_{}.png",
        Uuid::new_v4().simple()
    ));
    let mut args = Vec::new();
    if annotate {
        args.push("--annotate".to_string());
    }
    args.push("--full".to_string());
    args.push(screenshot_path.to_string_lossy().to_string());

    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "screenshot",
        args,
        ctx.command_timeout_seconds.max(60),
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to take screenshot"));
    }

    let actual_path = result
        .get("data")
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| screenshot_path.to_string_lossy().to_string());

    let (analysis, vision) = match analyze_screenshot_with_best_available_runtime(
        question.as_str(),
        actual_path.as_str(),
        conversation_id,
    )
    .await
    {
        Ok(output) => (
            output.analysis,
            json!({
                "enabled": true,
                "mode": output.mode,
                "prompt_source": output.prompt_source,
                "contact_agent_id": output.contact_agent_id,
                "model": output.model,
                "provider": output.provider,
                "transport": output.transport,
                "fallback_used": output.fallback_used,
                "transport_fallback_used": output.transport_fallback_used,
                "attempts": output.attempts,
                "warnings": output.warnings,
            }),
        ),
        Err(err) => (
            "Screenshot captured, but vision analysis was unavailable. See vision.error and vision.attempts.".to_string(),
            json!({
                "enabled": false,
                "mode": "unavailable",
                "error": err.error,
                "attempts": err.attempts,
                "warnings": err.warnings,
            }),
        ),
    };

    Ok(json!({
        "_summary_text": format!(
            "Captured a browser screenshot and produced vision analysis (vision available: {}, mode: {}, transport: {}).",
            if vision.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                "yes"
            } else {
                "no"
            },
            vision.get("mode").and_then(|v| v.as_str()).unwrap_or("unknown"),
            vision
                .get("transport")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        ),
        "success": true,
        "analysis": analysis,
        "question": question,
        "screenshot_path": actual_path,
        "annotations": result.get("data").and_then(|v| v.get("annotations")).cloned().unwrap_or(Value::Null),
        "vision": vision,
    }))
}

#[derive(Debug, Clone)]
struct BrowserVisionOutput {
    analysis: String,
    mode: String,
    prompt_source: String,
    contact_agent_id: Option<String>,
    model: String,
    provider: String,
    transport: String,
    fallback_used: bool,
    transport_fallback_used: bool,
    attempts: Vec<Value>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct BrowserVisionFailure {
    error: String,
    attempts: Vec<Value>,
    warnings: Vec<String>,
}

async fn analyze_screenshot_with_best_available_runtime(
    question: &str,
    screenshot_path: &str,
    conversation_id: Option<&str>,
) -> Result<BrowserVisionOutput, BrowserVisionFailure> {
    let prepared = prepare_browser_vision_context(conversation_id).await;
    let mut warnings = prepared.warnings.clone();
    let candidates = build_browser_vision_candidates(&prepared, &mut warnings).await;
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
                    "transport_fallback_used": run_result.transport_fallback_used,
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
                    transport_fallback_used: run_result.transport_fallback_used,
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
                    "transport": preferred_browser_vision_transport(&candidate).as_str(),
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
