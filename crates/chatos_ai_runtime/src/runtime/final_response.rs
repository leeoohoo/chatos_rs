// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::request::AiResponse;

use super::report::AiRuntimeResult;

pub(super) fn runtime_result_from_response(response: AiResponse) -> AiRuntimeResult {
    AiRuntimeResult {
        content: response.content,
        reasoning: response.reasoning,
        tool_calls: response.tool_calls,
        finish_reason: response.finish_reason,
        usage: response.usage,
        response_id: response.response_id,
        response_output_items: response.response_output_items,
        request_input_items: Vec::new(),
    }
}
