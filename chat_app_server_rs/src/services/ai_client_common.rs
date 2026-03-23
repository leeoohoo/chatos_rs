use std::sync::Arc;

use serde_json::Value;

#[derive(Clone, Default)]
pub struct AiClientCallbacks {
    pub on_chunk: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_tools_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_stream: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_stream: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
}

impl AiClientCallbacks {
    pub fn without_tool_callbacks(self) -> Self {
        Self {
            on_chunk: self.on_chunk,
            on_thinking: self.on_thinking,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_context_summarized_start: self.on_context_summarized_start,
            on_context_summarized_stream: self.on_context_summarized_stream,
            on_context_summarized_end: self.on_context_summarized_end,
        }
    }
}
