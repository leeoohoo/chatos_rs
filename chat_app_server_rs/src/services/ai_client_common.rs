use std::sync::Arc;

use serde_json::Value;

use crate::modules::conversation_runtime::snapshot::LiveRequestSnapshotContext;

#[derive(Clone, Default)]
pub struct AiClientCallbacks {
    pub on_chunk: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_turn_phase: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_stream: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_runtime_guidance_applied: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_stream: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_before_send_model_request: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_before_model_request: Option<
        Arc<dyn Fn(Value, Option<String>, Option<LiveRequestSnapshotContext>) + Send + Sync>,
    >,
}

impl AiClientCallbacks {
    pub fn without_tool_callbacks(self) -> Self {
        Self {
            on_chunk: self.on_chunk,
            on_thinking: self.on_thinking,
            on_turn_phase: self.on_turn_phase,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_runtime_guidance_applied: self.on_runtime_guidance_applied,
            on_context_summarized_start: self.on_context_summarized_start,
            on_context_summarized_stream: self.on_context_summarized_stream,
            on_context_summarized_end: self.on_context_summarized_end,
            on_before_send_model_request: self.on_before_send_model_request,
            on_before_model_request: self.on_before_model_request,
        }
    }
}
