// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct AiResponse {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub provider_error: Option<Value>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
}

#[derive(Clone, Default)]
pub struct StreamCallbacks {
    pub on_chunk: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTransport {
    Responses,
    ChatCompletions,
}

#[derive(Clone, Debug)]
pub struct AiRequestOptions {
    pub prompt_cache_key: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: bool,
    pub request_body_limit_bytes: Option<usize>,
    pub abort_token: Option<CancellationToken>,
    pub force_identity_encoding: bool,
    /// Controls the provider protocol independently from whether callers
    /// subscribe to incremental callbacks. Recovery requests can disable
    /// streaming for OpenAI-compatible gateways that truncate SSE bodies.
    pub stream: bool,
}

impl Default for AiRequestOptions {
    fn default() -> Self {
        Self {
            prompt_cache_key: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            abort_token: None,
            force_identity_encoding: false,
            stream: true,
        }
    }
}
