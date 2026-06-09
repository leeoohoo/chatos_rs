#[cfg(test)]
use chatos_ai_runtime::response_parse::join_stream_text;
#[cfg(test)]
use chatos_ai_runtime::response_parse::looks_like_response_id;
#[cfg(test)]
pub(super) use chatos_ai_runtime::response_parse::{
    extract_output_text, extract_reasoning_from_response,
};
pub(super) use chatos_ai_runtime::stream_parse::{
    apply_responses_stream_event as apply_stream_event, StreamState,
};
#[cfg(test)]
pub(super) use chatos_ai_runtime::stream_parse::{
    collect_stream_tool_calls, extract_responses_tool_calls as extract_tool_calls,
};

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn extract_tool_calls_collects_function_call_output_items() {
        let response = json!({
            "output": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "mcp.search",
                    "arguments": {"q": "rust"}
                },
                {
                    "type": "function_call",
                    "id": "call_2",
                    "name": "mcp.read",
                    "arguments": "{\"path\":\"README.md\"}"
                },
                {
                    "type": "message",
                    "content": "ignored"
                }
            ]
        });

        let tool_calls = extract_tool_calls(&response)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();

        assert_eq!(tool_calls.len(), 2);
        assert_eq!(
            tool_calls[0]
                .get("function")
                .and_then(|value| value.get("arguments"))
                .and_then(|value| value.as_str()),
            Some("{\"q\":\"rust\"}")
        );
        assert_eq!(
            tool_calls[1].get("id").and_then(|value| value.as_str()),
            Some("call_2")
        );
    }

    #[test]
    fn extract_output_text_reads_output_message_parts() {
        let response = json!({
            "output": [
                {
                    "type": "message",
                    "content": [
                        {"type": "output_text", "text": "Hello"},
                        {"type": "text", "text": " world"}
                    ]
                }
            ]
        });

        assert_eq!(extract_output_text(&response), "Hello world");
    }

    #[test]
    fn extract_output_text_handles_nested_text_value_objects() {
        let response = json!({
            "output_text": { "value": "Nested text" }
        });

        assert_eq!(extract_output_text(&response), "Nested text");
    }

    #[test]
    fn extract_reasoning_from_response_collects_reasoning_summary_items() {
        let response = json!({
            "output": [
                {
                    "type": "reasoning",
                    "summary": [{"text": "step-1"}]
                },
                {
                    "type": "reasoning_summary",
                    "text": "step-2"
                }
            ]
        });

        assert_eq!(extract_reasoning_from_response(&response), "step-1step-2");
    }

    #[test]
    fn looks_like_response_id_filters_event_ids_and_accepts_response_prefix() {
        assert!(looks_like_response_id("resp_abc"));
        assert!(looks_like_response_id("chatcmpl-123"));
        assert!(!looks_like_response_id("event_123"));
        assert!(!looks_like_response_id("call_123"));
        assert!(!looks_like_response_id("   "));
    }

    #[test]
    fn apply_stream_event_updates_stream_state_and_payload() {
        let mut state = StreamState::default();
        let event = json!({
            "type": "response.output_text.delta",
            "delta": {"text": "hello"},
            "response": {
                "id": "resp_1",
                "usage": {"total_tokens": 9}
            }
        });

        let payload = apply_stream_event(&mut state, &event);

        assert_eq!(payload.chunk.as_deref(), Some("hello"));
        assert_eq!(payload.thinking, None);
        assert_eq!(state.full_content, "hello");
        assert!(state.sent_any_chunk);
        assert_eq!(state.response_id.as_deref(), Some("resp_1"));
        assert!(state.usage.is_some());
    }

    #[test]
    fn apply_stream_event_merges_snapshot_style_delta_without_duplication() {
        let mut state = StreamState::default();

        let first = json!({
            "type": "response.output_text.delta",
            "delta": {"text": "严"},
        });
        let second = json!({
            "type": "response.output_text.delta",
            "delta": {"text": "严格式说：不是“真并行”。"},
        });

        let _ = apply_stream_event(&mut state, &first);
        let _ = apply_stream_event(&mut state, &second);

        assert_eq!(state.full_content, "严格式说：不是“真并行”。");
    }

    #[test]
    fn join_stream_text_handles_unicode_overlap_without_panic() {
        let current = "你好世界ABCD";
        let chunk = "好世界ABCD123";

        assert_eq!(join_stream_text(current, chunk), "你好世界ABCD123");
    }

    #[test]
    fn apply_stream_event_captures_provider_error_from_failed_event() {
        let mut state = StreamState::default();
        let event = json!({
            "type": "response.failed",
            "response": {
                "id": "resp_fail",
                "status": "failed",
                "error": {
                    "code": "context_length_exceeded",
                    "message": "too long"
                }
            }
        });

        let _ = apply_stream_event(&mut state, &event);

        assert_eq!(state.response_id.as_deref(), Some("resp_fail"));
        assert_eq!(state.finish_reason.as_deref(), Some("failed"));
        assert_eq!(
            state
                .provider_error
                .as_ref()
                .and_then(|value| value.get("code"))
                .and_then(|value| value.as_str()),
            Some("context_length_exceeded")
        );
    }

    #[test]
    fn apply_stream_event_marks_failed_finish_reason_without_status() {
        let mut state = StreamState::default();
        let event = json!({
            "type": "response.failed",
            "response": {
                "id": "resp_failed_no_status",
                "error": {
                    "message": "No tool call found for function_call_output item"
                }
            }
        });

        let _ = apply_stream_event(&mut state, &event);

        assert_eq!(state.response_id.as_deref(), Some("resp_failed_no_status"));
        assert_eq!(state.finish_reason.as_deref(), Some("failed"));
        assert_eq!(
            state
                .provider_error
                .as_ref()
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str()),
            Some("No tool call found for function_call_output item")
        );
    }

    #[test]
    fn apply_stream_event_handles_plain_response_object_without_type() {
        let mut state = StreamState::default();
        let event = json!({
            "id": "resp_plain",
            "output_text": "plain summary text",
            "status": "completed"
        });

        let payload = apply_stream_event(&mut state, &event);

        assert_eq!(payload.chunk.as_deref(), Some("plain summary text"));
        assert_eq!(state.response_id.as_deref(), Some("resp_plain"));
        assert_eq!(state.full_content, "plain summary text");
        assert!(state.response_obj.is_some());
    }

    #[test]
    fn apply_stream_event_collects_function_calls_from_stream_events() {
        let mut state = StreamState::default();
        let added = json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {
                "id": "fc_item_1",
                "type": "function_call",
                "call_id": "call_1",
                "name": "mcp_search",
                "arguments": ""
            }
        });
        let args_delta = json!({
            "type": "response.function_call_arguments.delta",
            "item_id": "fc_item_1",
            "delta": "{\"q\":\"ru"
        });
        let args_done = json!({
            "type": "response.function_call_arguments.done",
            "item_id": "fc_item_1",
            "arguments": "{\"q\":\"rust\"}"
        });

        let _ = apply_stream_event(&mut state, &added);
        let _ = apply_stream_event(&mut state, &args_delta);
        let _ = apply_stream_event(&mut state, &args_done);

        let tool_calls = collect_stream_tool_calls(&state.tool_calls_map)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(
            tool_calls[0].get("id").and_then(|value| value.as_str()),
            Some("call_1")
        );
        assert_eq!(
            tool_calls[0]
                .get("function")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str()),
            Some("mcp_search")
        );
        assert_eq!(
            tool_calls[0]
                .get("function")
                .and_then(|value| value.get("arguments"))
                .and_then(|value| value.as_str()),
            Some("{\"q\":\"rust\"}")
        );
    }

    #[test]
    fn apply_stream_event_collects_function_calls_from_response_completed() {
        let mut state = StreamState::default();
        let completed = json!({
            "type": "response.completed",
            "response": {
                "id": "resp_tool",
                "output": [
                    {
                        "type": "function_call",
                        "output_index": 0,
                        "id": "fc_item_2",
                        "call_id": "call_2",
                        "name": "mcp_read",
                        "arguments": "{\"path\":\"README.md\"}"
                    }
                ]
            }
        });

        let _ = apply_stream_event(&mut state, &completed);
        let tool_calls = collect_stream_tool_calls(&state.tool_calls_map)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(
            tool_calls[0].get("id").and_then(|value| value.as_str()),
            Some("call_2")
        );
    }
}
