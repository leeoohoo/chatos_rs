// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::JsonSchemaOutputFormat;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiResponse {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub provider_error: Option<Value>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
    /// Provider response lifecycle state. Responses API values include
    /// `completed`, `incomplete`, and `failed`.
    #[serde(default)]
    pub response_status: Option<String>,
    /// Structured reason supplied by an incomplete Responses API result.
    #[serde(default)]
    pub incomplete_details: Option<Value>,
    /// Final SSE event observed for this response (`response.completed`,
    /// `response.incomplete`, or `response.failed`).
    #[serde(default)]
    pub terminal_event_type: Option<String>,
    #[serde(default)]
    pub terminal_event_seen: bool,
    /// OpenAI's request correlation header, when supplied by the provider.
    #[serde(default)]
    pub provider_request_id: Option<String>,
    #[serde(default)]
    pub provider_http_status: Option<u16>,
    #[serde(default)]
    pub parsed_stream_event_count: usize,
    #[serde(default)]
    pub malformed_stream_event_count: usize,
    /// Exact `response.output` items returned by the Responses API.
    ///
    /// Cloud event-driven callers persist these items verbatim and append
    /// tool outputs before the next request, as required by the official
    /// stateless Responses function-calling protocol. Chat Completions leaves
    /// this empty.
    #[serde(default)]
    pub response_output_items: Vec<Value>,
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
    pub previous_response_id: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: bool,
    pub request_body_limit_bytes: Option<usize>,
    pub abort_token: Option<CancellationToken>,
    pub force_identity_encoding: bool,
    /// Controls the provider protocol independently from whether callers
    /// subscribe to incremental callbacks. Recovery requests can disable
    /// streaming for OpenAI-compatible gateways that truncate SSE bodies.
    pub stream: bool,
    pub output_format: Option<JsonSchemaOutputFormat>,
    /// Enables Responses server-side compaction at this rendered-token
    /// threshold. This is only sent to the official OpenAI API.
    pub responses_compaction_threshold: Option<usize>,
}

impl Default for AiRequestOptions {
    fn default() -> Self {
        Self {
            prompt_cache_key: None,
            previous_response_id: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            abort_token: None,
            force_identity_encoding: false,
            stream: true,
            output_format: None,
            responses_compaction_threshold: None,
        }
    }
}
