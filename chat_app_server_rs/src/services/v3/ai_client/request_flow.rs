use serde_json::Value;
use tracing::info;

use super::input_transform::{
    build_current_input_items, extract_raw_input, normalize_input_for_provider,
};
use super::prev_context::{
    model_supports_prev_response_id, should_disable_prev_id_for_prefixed_input_items,
    should_prefer_stateless_context,
};
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
        let system_prompt = options.system_prompt.or_else(|| self.system_prompt.clone());
        let history_limit = options.history_limit.unwrap_or(self.history_limit);
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

        // Responses-capable models can continue turns via previous_response_id. For legacy
        // chat-completions style models, chat mode keeps the bounded stateless prefix behavior.
        let prefer_stateless =
            should_prefer_stateless_context(&purpose, supports_responses, history_limit);
        info!(
            "[AI_V3][prev-id] request begin: session_id={}, purpose={}, supports_responses={}, prefer_stateless={}, history_limit={}",
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            purpose,
            supports_responses,
            prefer_stateless,
            history_limit
        );
        let mut previous_response_id: Option<String> = None;
        if !prefer_stateless {
            if let Some(sid) = session_id.as_ref() {
                let limit = if history_limit > 0 {
                    Some(history_limit)
                } else {
                    None
                };
                previous_response_id = self
                    .message_manager
                    .get_last_response_id(sid, limit.unwrap_or(50))
                    .await;
                info!(
                    "[AI_V3][prev-id] fetched previous response: session_id={}, response_id={}",
                    sid,
                    previous_response_id.as_deref().unwrap_or("none")
                );
            }
        } else {
            info!(
                "[AI_V3][prev-id] skipped fetch because stateless mode is preferred: session_id={}",
                session_id.clone().unwrap_or_else(|| "n/a".to_string())
            );
        }

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
            "[AI_V3] tools prepared: count={}, session={}, codex_gateway_passthrough={}",
            available_tools.len(),
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            use_codex_gateway_mcp_passthrough
        );

        let allow_prev_id = session_id
            .as_ref()
            .map(|s| !self.prev_response_id_disabled_sessions.contains(s))
            .unwrap_or(true);
        let mut can_use_prev_id =
            allow_prev_id && model_supports_prev_response_id(supports_responses);
        if can_use_prev_id
            && should_disable_prev_id_for_prefixed_input_items(prefixed_input_items.as_slice())
        {
            info!(
                "[AI_V3] disable previous_response_id because runtime prefixed input items are present: session_id={}",
                session_id.clone().unwrap_or_else(|| "n/a".to_string())
            );
            can_use_prev_id = false;
            previous_response_id = None;
        }
        let use_prev_id = !prefer_stateless && previous_response_id.is_some() && can_use_prev_id;
        let stateless_history_limit = if !use_prev_id && history_limit == 0 {
            tracing::warn!("[AI_V3] history_limit=0 with stateless mode; fallback to 20");
            20
        } else {
            history_limit
        };
        info!(
            "[AI_V3] context mode: session_id={}, use_prev_id={}, can_use_prev_id={}, supports_responses={}, provider={}, history_limit={}, has_prev_id={}, prev_response_id={}, stable_prefix_mode={}",
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            use_prev_id,
            can_use_prev_id,
            supports_responses,
            provider,
            stateless_history_limit,
            previous_response_id.is_some(),
            previous_response_id.as_deref().unwrap_or("none"),
            stable_prefix_mode
        );
        let initial_input = if use_prev_id {
            normalize_input_for_provider(&raw_input, force_text_content)
        } else {
            let current_items = build_current_input_items(&raw_input, force_text_content);
            Value::Array(
                self.build_stateless_items(
                    session_id.clone(),
                    stateless_history_limit,
                    stable_prefix_mode,
                    force_text_content,
                    prefixed_input_items.as_slice(),
                    &current_items,
                    include_tool_items,
                )
                .await,
            )
        };

        self.process_with_tools(
            initial_input,
            previous_response_id,
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
            use_prev_id,
            can_use_prev_id,
            raw_input,
            stateless_history_limit,
            stable_prefix_mode,
            force_text_content,
            prefixed_input_items,
            prefer_stateless,
            false,
            false,
            message_mode,
            message_source,
            request_cwd,
        )
        .await
    }
}
