use serde_json::{json, Value};

use crate::services::agent_runtime::ai_client::{AiClient, AiClientCallbacks, ProcessOptions};
use crate::services::agent_runtime::ai_request_handler::AiRequestHandler;
use crate::services::agent_runtime::mcp_tool_execute::McpToolExecute;
use crate::services::agent_runtime::message_manager::MessageManager;
use crate::services::ai_common::{normalize_turn_id, persist_user_message_and_build_content_parts};
use crate::services::shared_ai_runtime::{
    build_shared_ai_runtime_with_chatos_records, build_shared_contextual_turn_runner,
};
use crate::utils::attachments;

pub struct AiServer {
    pub message_manager: MessageManager,
    pub ai_request_handler: AiRequestHandler,
    pub mcp_tool_execute: McpToolExecute,
    pub ai_client: AiClient,
    pub shared_ai_runtime: chatos_ai_runtime::AiRuntime,
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
        let shared_ai_runtime = build_shared_ai_runtime_with_chatos_records(
            Some(mcp_tool_execute.clone()),
            message_manager.clone(),
        );
        Self {
            message_manager,
            ai_request_handler,
            mcp_tool_execute,
            ai_client,
            shared_ai_runtime,
            default_model,
            default_temperature,
            base_url,
        }
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.ai_client.set_system_prompt(prompt);
    }

    pub fn set_mcp_tool_execute(&mut self, mcp_tool_execute: McpToolExecute) {
        self.mcp_tool_execute = mcp_tool_execute.clone();
        self.ai_client
            .set_mcp_tool_execute(mcp_tool_execute.clone());
        self.shared_ai_runtime = build_shared_ai_runtime_with_chatos_records(
            Some(mcp_tool_execute),
            self.message_manager.clone(),
        );
    }

    pub fn build_shared_contextual_turn_runner(
        &self,
    ) -> Result<chatos_ai_runtime::ContextualTurnRunner, String> {
        build_shared_contextual_turn_runner(
            Some(self.mcp_tool_execute.clone()),
            self.message_manager.clone(),
        )
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
        let turn_id = normalize_turn_id(options.turn_id.as_deref());
        let user_message_id = options.user_message_id.clone();
        let message_mode = options.message_mode.clone();
        let message_source = options.message_source.clone();
        let prepared = persist_user_message_and_build_content_parts(
            session_id,
            user_message,
            model.as_str(),
            options.attachments.unwrap_or_default(),
            options.supports_images,
            turn_id,
            |metadata| {
                self.message_manager.save_user_message(
                    session_id,
                    user_message,
                    user_message_id,
                    message_mode,
                    message_source,
                    metadata,
                )
            },
        )
        .await?;
        let turn_id = prepared.turn_id;
        let content_parts = prepared.content_parts;

        let messages = vec![json!({"role": "user", "content": content_parts})];

        let callbacks = options.callbacks.unwrap_or_default();

        let result = self
            .ai_client
            .process_request(
                messages,
                Some(session_id.to_string()),
                ProcessOptions {
                    model: Some(model),
                    temperature: Some(temperature),
                    max_tokens,
                    reasoning_enabled: Some(reasoning_enabled),
                    supports_responses: options.supports_responses,
                    supports_images: options.supports_images,
                    provider: Some(provider),
                    thinking_level,
                    system_prompt: None,
                    purpose: Some("chat".to_string()),
                    conversation_turn_id: turn_id.clone(),
                    message_mode: options.message_mode.clone(),
                    message_source: options.message_source.clone(),
                    prefixed_input_items: options.prefixed_input_items.clone(),
                    request_cwd: options.request_cwd.clone(),
                    use_codex_gateway_mcp_passthrough: options.use_codex_gateway_mcp_passthrough,
                    callbacks: Some(if use_tools {
                        callbacks
                    } else {
                        callbacks.without_tool_callbacks()
                    }),
                },
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
    pub supports_responses: Option<bool>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub use_tools: Option<bool>,
    pub attachments: Option<Vec<attachments::Attachment>>,
    pub supports_images: Option<bool>,
    pub reasoning_enabled: Option<bool>,
    pub callbacks: Option<AiClientCallbacks>,
    pub turn_id: Option<String>,
    pub user_message_id: Option<String>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub prefixed_input_items: Option<Vec<Value>>,
    pub request_cwd: Option<String>,
    pub use_codex_gateway_mcp_passthrough: Option<bool>,
}
