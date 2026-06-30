use async_trait::async_trait;
use serde_json::json;

use chatos_builtin_tools::{
    BrowserVisionAdapter, BrowserVisionFailure, BrowserVisionRequest, BrowserVisionResponse,
};

mod candidates;
mod context;
mod runner;
mod support;
mod types;

use runner::analyze_screenshot_with_best_available_runtime;

pub(crate) struct ChatosBrowserVisionAdapter;

#[async_trait]
impl BrowserVisionAdapter for ChatosBrowserVisionAdapter {
    async fn analyze_screenshot(
        &self,
        request: BrowserVisionRequest,
    ) -> Result<BrowserVisionResponse, BrowserVisionFailure> {
        let output = analyze_screenshot_with_best_available_runtime(
            request.question.as_str(),
            request.screenshot_path.as_str(),
            request.conversation_id.as_deref(),
            request.caller_model_runtime.as_ref(),
        )
        .await?;

        Ok(BrowserVisionResponse {
            analysis: output.analysis,
            vision: json!({
                "enabled": true,
                "mode": output.mode,
                "prompt_source": output.prompt_source,
                "contact_agent_id": output.contact_agent_id,
                "model": output.model,
                "provider": output.provider,
                "transport": output.transport,
                "fallback_used": output.fallback_used,
                "attempts": output.attempts,
                "warnings": output.warnings,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use chatos_mcp_runtime::ToolCallerModelRuntime;

    use super::candidates::browser_vision_candidate_from_caller_runtime;

    #[test]
    fn caller_runtime_builds_first_class_browser_vision_candidate() {
        let runtime = ToolCallerModelRuntime::openai_compatible(
            "https://models.example/v1",
            "secret",
            "custom-vision-model",
            "custom",
        )
        .with_responses_support(true)
        .with_images_support(Some(true))
        .with_temperature(Some(0.2))
        .with_thinking_level(Some("low".to_string()));

        let candidate = browser_vision_candidate_from_caller_runtime(
            &runtime,
            "contact_agent",
            Some("agent_1".to_string()),
            Some("contact prompt".to_string()),
        )
        .expect("caller runtime candidate");

        assert_eq!(candidate.mode, "caller_model");
        assert_eq!(candidate.prompt_source, "contact_agent");
        assert_eq!(candidate.contact_agent_id.as_deref(), Some("agent_1"));
        assert_eq!(candidate.model, "custom-vision-model");
        assert_eq!(candidate.provider, "custom");
        assert_eq!(candidate.base_url, "https://models.example/v1");
        assert_eq!(candidate.temperature, 0.2);
        assert_eq!(candidate.thinking_level.as_deref(), Some("low"));
    }

    #[test]
    fn caller_runtime_rejects_non_responses_or_non_vision_models() {
        let non_responses = ToolCallerModelRuntime::openai_compatible(
            "https://models.example/v1",
            "secret",
            "custom-vision-model",
            "custom",
        )
        .with_responses_support(false)
        .with_images_support(Some(true));
        assert!(browser_vision_candidate_from_caller_runtime(
            &non_responses,
            "generic",
            None,
            None,
        )
        .is_none());

        let non_vision = ToolCallerModelRuntime::openai_compatible(
            "https://models.example/v1",
            "secret",
            "text-only-model",
            "custom",
        )
        .with_responses_support(true)
        .with_images_support(Some(false));
        assert!(
            browser_vision_candidate_from_caller_runtime(&non_vision, "generic", None, None,)
                .is_none()
        );
    }
}
