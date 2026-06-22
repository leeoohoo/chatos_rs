use serde_json::Value;
use tracing::info;

use super::input_transform::{build_current_input_items, extract_raw_input};
use super::{AiClient, ProcessOptions};

impl AiClient {
    pub async fn process_request(
        &mut self,
        messages: Vec<Value>,
        session_id: Option<String>,
        options: ProcessOptions,
    ) -> Result<Value, String> {
        let model = options.model.unwrap_or_else(|| "gpt-4o".to_string());
        let provider = options.provider.unwrap_or_else(|| "gpt".to_string());
        let thinking_level = options.thinking_level.clone();
        let temperature = options.temperature.unwrap_or(0.7);
        let max_tokens = options.max_tokens;
        let reasoning_enabled = options.reasoning_enabled.unwrap_or(true);
        let supports_responses = options.supports_responses.unwrap_or(false);
        let supports_images = options.supports_images;
        let system_prompt = options.system_prompt.or_else(|| self.system_prompt.clone());
        let purpose = options.purpose.unwrap_or_else(|| "chat".to_string());
        let message_mode = options.message_mode;
        let message_source = options.message_source;
        let prefixed_input_items = options.prefixed_input_items.unwrap_or_default();
        let request_cwd = options.request_cwd;
        let use_codex_gateway_mcp_passthrough =
            options.use_codex_gateway_mcp_passthrough.unwrap_or(false);
        let turn_id = options
            .conversation_turn_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let callbacks = options.callbacks.unwrap_or_default();
        let stable_prefix_mode = purpose == "chat";
        let prompt_cache_key = self.build_prompt_cache_key(&purpose, session_id.as_deref());

        info!(
            "[Agent Runtime] stateless context mode: session_id={}, purpose={}, supports_responses={}",
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            purpose,
            supports_responses
        );

        let raw_input = extract_raw_input(&messages);
        let force_text_content = session_id
            .as_ref()
            .map(|s| self.force_text_content_sessions.contains(s))
            .unwrap_or(false);
        let available_tools = if use_codex_gateway_mcp_passthrough {
            self.mcp_tool_execute.get_codex_gateway_request_tools()
        } else {
            self.mcp_tool_execute.get_available_tools()
        };
        let include_tool_items = !available_tools.is_empty();
        info!(
            "[Agent Runtime] tools prepared: count={}, session={}, codex_gateway_passthrough={}",
            available_tools.len(),
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            use_codex_gateway_mcp_passthrough
        );

        let current_items = build_current_input_items(&raw_input, force_text_content);
        let initial_input = Value::Array(
            self.build_stateless_items(
                session_id.clone(),
                stable_prefix_mode,
                force_text_content,
                prefixed_input_items.as_slice(),
                &current_items,
                include_tool_items,
            )
            .await,
        );

        self.process_with_tools(
            initial_input,
            prompt_cache_key,
            available_tools,
            session_id,
            turn_id,
            model,
            provider,
            thinking_level,
            temperature,
            max_tokens,
            callbacks,
            reasoning_enabled,
            system_prompt,
            &purpose,
            0,
            raw_input,
            stable_prefix_mode,
            force_text_content,
            prefixed_input_items,
            false,
            false,
            message_mode,
            message_source,
            request_cwd,
            supports_responses,
            supports_images,
        )
        .await
    }
}
