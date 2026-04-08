use std::collections::HashSet;

use serde_json::Value;

pub use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::user_settings::AiClientSettings;
use crate::services::v3::ai_request_handler::AiRequestHandler;
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::services::v3::message_manager::MessageManager;

mod compat;
mod execution_loop;
mod input_transform;
mod prev_context;
mod recovery_policy;
mod request_flow;
mod stateless_context;
#[cfg(test)]
mod test_support;
mod tool_plan;

use self::compat::truncate_function_call_outputs_in_input;
use self::input_transform::{
    build_current_input_items, normalize_input_to_text_value, to_message_item,
};

#[derive(Default)]
pub struct ProcessOptions {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub reasoning_enabled: Option<bool>,
    pub supports_responses: Option<bool>,
    pub system_prompt: Option<String>,
    pub history_limit: Option<i64>,
    pub purpose: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub prefixed_input_items: Option<Vec<Value>>,
    pub request_cwd: Option<String>,
    pub use_codex_gateway_mcp_passthrough: Option<bool>,
    pub callbacks: Option<AiClientCallbacks>,
}

pub struct AiClient {
    ai_request_handler: AiRequestHandler,
    mcp_tool_execute: McpToolExecute,
    message_manager: MessageManager,
    max_iterations: i64,
    history_limit: i64,
    system_prompt: Option<String>,
    prev_response_id_disabled_sessions: HashSet<String>,
    force_text_content_sessions: HashSet<String>,
    no_system_message_sessions: HashSet<String>,
}

impl AiClient {
    pub fn new(
        ai_request_handler: AiRequestHandler,
        mcp_tool_execute: McpToolExecute,
        message_manager: MessageManager,
    ) -> Self {
        Self {
            ai_request_handler,
            mcp_tool_execute,
            message_manager,
            max_iterations: 25,
            history_limit: 20,
            system_prompt: None,
            prev_response_id_disabled_sessions: HashSet::new(),
            force_text_content_sessions: HashSet::new(),
            no_system_message_sessions: HashSet::new(),
        }
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.system_prompt = prompt;
    }

    pub fn set_mcp_tool_execute(&mut self, mcp_tool_execute: McpToolExecute) {
        self.mcp_tool_execute = mcp_tool_execute;
    }
}

impl AiClientSettings for AiClient {
    fn apply_settings(&mut self, effective: &Value) {
        if let Some(v) = effective.get("MAX_ITERATIONS").and_then(|v| v.as_i64()) {
            self.max_iterations = v;
        }
        if let Some(v) = effective.get("HISTORY_LIMIT").and_then(|v| v.as_i64()) {
            self.history_limit = v.max(0);
        }
    }
}

#[cfg(test)]
mod tests;
