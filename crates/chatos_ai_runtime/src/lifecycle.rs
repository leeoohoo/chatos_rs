// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde_json::Value;

use crate::AiResponse;

#[derive(Debug, Clone)]
pub struct RuntimeIterationContext {
    pub conversation_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub iteration: usize,
    pub reason: String,
    pub input: Value,
}

#[derive(Debug, Clone)]
pub struct RuntimeBeforeModelRequest {
    pub input_items: Vec<Value>,
    pub stream_output: bool,
    pub tools_enabled: bool,
}

impl RuntimeBeforeModelRequest {
    pub fn unchanged() -> Self {
        Self {
            input_items: Vec::new(),
            stream_output: true,
            tools_enabled: true,
        }
    }

    pub fn with_input_items(mut self, input_items: Vec<Value>) -> Self {
        self.input_items = input_items;
        self
    }

    pub fn with_stream_output(mut self, stream_output: bool) -> Self {
        self.stream_output = stream_output;
        self
    }

    pub fn with_tools_enabled(mut self, tools_enabled: bool) -> Self {
        self.tools_enabled = tools_enabled;
        self
    }
}

impl Default for RuntimeBeforeModelRequest {
    fn default() -> Self {
        Self::unchanged()
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeFinalResponseContext {
    pub conversation_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub iteration: usize,
    pub reason: String,
    pub response: AiResponse,
}

#[derive(Debug, Clone)]
pub enum RuntimeFinalResponseAction {
    Accept,
    Continue {
        input_items: Vec<Value>,
        reason: String,
    },
    Replace(AiResponse),
}

#[async_trait]
pub trait RuntimeLifecycleHook: Send + Sync {
    async fn before_model_request(
        &self,
        _context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        Ok(RuntimeBeforeModelRequest::unchanged())
    }

    async fn after_final_response(
        &self,
        _context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
        Ok(RuntimeFinalResponseAction::Accept)
    }

    async fn final_response_metadata(
        &self,
        _context: RuntimeFinalResponseContext,
    ) -> Result<Option<Value>, String> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DefaultHook;

    #[async_trait]
    impl RuntimeLifecycleHook for DefaultHook {}

    #[tokio::test]
    async fn default_hook_keeps_runtime_behavior_unchanged() {
        let before = DefaultHook
            .before_model_request(RuntimeIterationContext {
                conversation_id: Some("session-1".to_string()),
                conversation_turn_id: Some("turn-1".to_string()),
                iteration: 1,
                reason: "initial".to_string(),
                input: Value::Null,
            })
            .await
            .expect("before hook");

        assert!(before.input_items.is_empty());
        assert!(before.stream_output);
        assert!(before.tools_enabled);
    }
}
