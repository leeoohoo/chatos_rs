// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde_json::Value;

use crate::modules::conversation_runtime::snapshot::LiveRequestSnapshotContext;

type BeforeModelRequestCallback =
    Arc<dyn Fn(&Value, Option<String>, Option<LiveRequestSnapshotContext>) + Send + Sync>;

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
    pub on_before_model_request: Option<BeforeModelRequestCallback>,
}
