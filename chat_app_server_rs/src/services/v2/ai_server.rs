use serde_json::{json, Value};
use tracing::warn;

use crate::services::v2::ai_client::{AiClient, AiClientCallbacks};
use crate::services::v2::ai_request_handler::AiRequestHandler;
use crate::services::v2::mcp_tool_execute::McpToolExecute;
use crate::services::v2::message_manager::MessageManager;
use crate::utils::attachments;

pub struct AiServer {
    pub message_manager: MessageManager,
    pub ai_request_handler: AiRequestHandler,
    pub mcp_tool_execute: McpToolExecute,
    pub ai_client: AiClient,
    pub default_model: String,
    pub default_temperature: f64,
    pub base_url: String,
}

impl AiServer {
    pub fn new(
        openai_api_key: String,
        base_url: String,
        default_model: String,
        default_temperature: f64,
        mcp_tool_execute: McpToolExecute,
    ) -> Self {
        let message_manager = MessageManager::new();
        let ai_request_handler = AiRequestHandler::new(
            openai_api_key.clone(),
            base_url.clone(),
            message_manager.clone(),
        );
        let ai_client = AiClient::new(
            ai_request_handler.clone(),
            mcp_tool_execute.clone(),
            message_manager.clone(),
        );
        Self {
            message_manager,
            ai_request_handler,
            mcp_tool_execute,
            ai_client,
            default_model,
            default_temperature,
            base_url,
        }
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.ai_client.set_system_prompt(prompt);
    }

    pub async fn chat(
        &mut self,
        session_id: &str,
        user_message: &str,
        options: ChatOptions,
    ) -> Result<Value, String> {
        let model = options.model.unwrap_or_else(|| self.default_model.clone());
        let provider = options.provider.unwrap_or_else(|| "gpt".to_string());
        let thinking_level = options.thinking_level.clone();
        let temperature = options.temperature.unwrap_or(self.default_temperature);
        let use_tools = options.use_tools.unwrap_or(true);
        let max_tokens = options.max_tokens;
        let reasoning_enabled = options.reasoning_enabled.unwrap_or(true);
        let turn_id = options
            .turn_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());

        let attachments_list = options.attachments.unwrap_or_default();
        let sanitized = attachments::sanitize_attachments_for_db(&attachments_list);
        let meta = if sanitized.is_empty() && turn_id.is_none() {
            None
        } else {
            let mut map = serde_json::Map::new();
            if !sanitized.is_empty() {
                map.insert("attachments".to_string(), json!(sanitized));
            }
            if let Some(turn) = turn_id.clone() {
                map.insert("conversation_turn_id".to_string(), Value::String(turn));
            }
            Some(Value::Object(map))
        };
        if let Err(err) = self
            .message_manager
            .save_user_message(session_id, user_message, None, meta)
            .await
        {
            warn!("save user message failed: {}", err);
        }

        let mut content_parts =
            attachments::build_content_parts_async(user_message, &attachments_list).await;
        content_parts =
            attachments::adapt_parts_for_model(&model, &content_parts, options.supports_images);

        let messages = vec![json!({"role": "user", "content": content_parts})];

        let result = self
            .ai_client
            .process_request(
                messages,
                Some(session_id.to_string()),
                turn_id.clone(),
                model,
                temperature,
                max_tokens,
                use_tools,
                options.callbacks.unwrap_or_default(),
                reasoning_enabled,
                Some(provider),
                thinking_level,
                Some("chat".to_string()),
            )
            .await?;

        Ok(result)
    }
}

#[derive(Default)]
pub struct ChatOptions {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub use_tools: Option<bool>,
    pub attachments: Option<Vec<attachments::Attachment>>,
    pub supports_images: Option<bool>,
    pub reasoning_enabled: Option<bool>,
    pub callbacks: Option<AiClientCallbacks>,
    pub turn_id: Option<String>,
}

impl Default for AiClientCallbacks {
    fn default() -> Self {
        AiClientCallbacks {
            on_chunk: None,
            on_thinking: None,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_context_summarized_start: None,
            on_context_summarized_stream: None,
            on_context_summarized_end: None,
        }
    }
}
