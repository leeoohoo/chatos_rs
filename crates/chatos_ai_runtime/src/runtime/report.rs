// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AiRuntimeResult {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
    pub response_output_items: Vec<Value>,
    /// Exact input items used for this model request after Memory Engine
    /// composition. Cloud orchestration persists this as the base of the next
    /// stateless Responses request.
    pub request_input_items: Vec<Value>,
}
