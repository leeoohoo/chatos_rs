use std::collections::HashSet;

use serde_json::Value;

use crate::core::ai_settings::request_body_limit_bytes_from_settings;
#[cfg(test)]
use crate::core::internal_context_locale::InternalContextLocale;
use crate::services::agent_runtime::ai_request_handler::AiRequestHandler;
use crate::services::agent_runtime::mcp_tool_execute::McpToolExecute;
use crate::services::agent_runtime::message_manager::MessageManager;
pub use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::task_board_refresh_context::TaskBoardRefreshContextStore;
use crate::services::user_settings::AiClientSettings;

mod compat;
mod execution_loop;
mod execution_loop_follow_up;
mod execution_loop_guidance;
mod execution_loop_state;
mod execution_loop_tool_io;
mod input_transform;
mod prev_context;
mod recovery_policy;
mod request_flow;
mod stateless_context;
#[cfg(test)]
mod test_support;
mod tool_plan;

use self::input_transform::{build_current_input_items, normalize_input_to_text_value};

#[derive(Default)]
pub struct ProcessOptions {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub reasoning_enabled: Option<bool>,
    pub supports_responses: Option<bool>,
    pub supports_images: Option<bool>,
    pub system_prompt: Option<String>,
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
    task_follow_up_max_rounds: usize,
    request_body_limit_bytes: Option<usize>,
    system_prompt: Option<String>,
    force_text_content_sessions: HashSet<String>,
    no_system_message_sessions: HashSet<String>,
    task_board_refresh_context: TaskBoardRefreshContextStore,
}

impl AiClient {
    pub(super) fn build_prompt_cache_key(
        &self,
        purpose: &str,
        session_id: Option<&str>,
    ) -> Option<String> {
        if purpose != "chat" {
            return None;
        }

        session_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
    }

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
            task_follow_up_max_rounds: 3,
            request_body_limit_bytes: Some(request_body_limit_bytes_from_settings(&Value::Null)),
            system_prompt: None,
            force_text_content_sessions: HashSet::new(),
            no_system_message_sessions: HashSet::new(),
            task_board_refresh_context: TaskBoardRefreshContextStore::new(),
        }
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.system_prompt = prompt;
    }

    pub fn set_mcp_tool_execute(&mut self, mcp_tool_execute: McpToolExecute) {
        self.mcp_tool_execute = mcp_tool_execute;
    }

    #[cfg(test)]
    pub fn set_task_board_refresh_context(
        &mut self,
        session_id: Option<String>,
        turn_id: Option<String>,
        locale: InternalContextLocale,
        contact_system_prompt: Option<String>,
        builtin_mcp_system_prompt: Option<String>,
        command_system_prompt: Option<String>,
        task_runner_skill_prompt: Option<String>,
    ) {
        self.task_board_refresh_context.set(
            session_id,
            turn_id,
            locale,
            contact_system_prompt,
            builtin_mcp_system_prompt,
            command_system_prompt,
            task_runner_skill_prompt,
        );
    }

    pub async fn load_runtime_prefixed_input_items(&self) -> Option<Vec<Value>> {
        self.task_board_refresh_context
            .load_prefixed_input_items()
            .await
    }
}

impl AiClientSettings for AiClient {
    fn apply_settings(&mut self, effective: &Value) {
        if let Some(v) = effective.get("MAX_ITERATIONS").and_then(|v| v.as_i64()) {
            self.max_iterations = v;
        }
        if let Some(v) = effective
            .get("TASK_FOLLOW_UP_MAX_ROUNDS")
            .and_then(|v| v.as_i64())
        {
            self.task_follow_up_max_rounds = v.max(0) as usize;
        }
        self.request_body_limit_bytes = Some(request_body_limit_bytes_from_settings(effective));
    }
}

#[cfg(test)]
mod tests;
