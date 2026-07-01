// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::parsing::{
    extract_chat_completion_stream_text, extract_chat_completion_text,
    extract_responses_output_text, extract_responses_stream_text, extract_stream_error_message,
};
use super::protocol::{
    effective_request_temperature, provider_requires_disabled_thinking,
    provider_requires_unit_temperature,
};

#[test]
fn moonshot_kimi_forces_unit_temperature() {
    assert!(provider_requires_unit_temperature(
        "https://api.moonshot.cn/v1",
        "kimi-k2.6"
    ));
    assert_eq!(
        effective_request_temperature("https://api.moonshot.cn/v1", "kimi-k2.6", 0.2),
        0.6
    );
}

#[test]
fn moonshot_kimi_disables_thinking() {
    assert!(provider_requires_disabled_thinking(
        "https://api.moonshot.cn/v1",
        "kimi-k2.6"
    ));
}

#[test]
fn generic_openai_profile_keeps_configured_temperature() {
    assert!(!provider_requires_unit_temperature(
        "https://api.openai.com/v1",
        "gpt-5.4"
    ));
    assert_eq!(
        effective_request_temperature("https://api.openai.com/v1", "gpt-5.4", 0.2),
        0.2
    );
}

#[test]
fn chat_completion_parser_accepts_string_content() {
    let value = json!({
        "choices": [
            {
                "message": {
                    "content": "hello"
                }
            }
        ]
    });
    assert_eq!(
        extract_chat_completion_text(&value).as_deref(),
        Some("hello")
    );
}

#[test]
fn chat_completion_parser_accepts_content_parts() {
    let value = json!({
        "choices": [
            {
                "message": {
                    "content": [
                        {"type": "text", "text": "hello"},
                        {"type": "text", "text": "world"}
                    ]
                }
            }
        ]
    });
    assert_eq!(
        extract_chat_completion_text(&value).as_deref(),
        Some("hello\nworld")
    );
}

#[test]
fn chat_completion_stream_parser_accepts_delta_content() {
    let value = json!({
        "choices": [
            {
                "delta": {
                    "content": "hello"
                }
            }
        ]
    });
    assert_eq!(
        extract_chat_completion_stream_text(&value).as_deref(),
        Some("hello")
    );
}

#[test]
fn responses_output_parser_accepts_output_text() {
    let value = json!({
        "output_text": "hello world"
    });
    assert_eq!(
        extract_responses_output_text(&value).as_deref(),
        Some("hello world")
    );
}

#[test]
fn responses_stream_parser_accepts_delta_event() {
    let value = json!({
        "type": "response.output_text.delta",
        "delta": "hello"
    });
    assert_eq!(
        extract_responses_stream_text(&value, false).as_deref(),
        Some("hello")
    );
}

#[test]
fn responses_stream_parser_accepts_completed_response() {
    let value = json!({
        "type": "response.completed",
        "response": {
            "output": [
                {
                    "type": "message",
                    "content": [
                        {
                            "type": "output_text",
                            "text": "merged summary"
                        }
                    ]
                }
            ]
        }
    });
    assert_eq!(
        extract_responses_stream_text(&value, false).as_deref(),
        Some("merged summary")
    );
    assert_eq!(extract_responses_stream_text(&value, true), None);
}

#[test]
fn stream_error_parser_accepts_response_failed_event() {
    let value = json!({
        "type": "response.failed",
        "response": {
            "error": {
                "message": "provider failure"
            }
        }
    });
    assert_eq!(
        extract_stream_error_message(&value).as_deref(),
        Some("provider failure")
    );
}
