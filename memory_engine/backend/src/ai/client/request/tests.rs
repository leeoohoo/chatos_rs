// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn assert_pure_generation_request(body: &Value) {
    for key in ["tools", "tool_choice", "functions", "function_call"] {
        assert!(body.get(key).is_none(), "unexpected tool field: {key}");
    }
}

#[test]
fn chat_completions_request_is_text_generation_only() {
    let body = build_chat_completions_body("model", "system", "user", Some(800), 0.2, false, false);

    assert_pure_generation_request(&body);
    assert_eq!(body["stream"], true);
    assert_eq!(body["messages"][0]["role"], "system");
}

#[test]
fn responses_request_is_text_generation_only() {
    let body = build_responses_body(
        "model",
        "system",
        "user",
        Some(800),
        0.2,
        false,
        false,
        false,
    );

    assert_pure_generation_request(&body);
    assert_eq!(body["stream"], true);
    assert_eq!(body["instructions"], "system");
}

#[test]
fn byte_buffer_preserves_multibyte_text_split_across_chunks() {
    let event = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"你好世界\"}\n\n";
    let bytes = event.as_bytes();
    let split_inside_multibyte_char = event.find('好').expect("test fixture has char") + 1;

    let mut buffer = Vec::new();
    buffer.extend_from_slice(&bytes[..split_inside_multibyte_char]);
    assert!(find_sse_event_delimiter(buffer.as_slice()).is_none());

    buffer.extend_from_slice(&bytes[split_inside_multibyte_char..]);
    let (index, delimiter_len) =
        find_sse_event_delimiter(buffer.as_slice()).expect("complete event delimiter");
    let raw_event = decode_sse_event_bytes(buffer[..index].to_vec()).expect("valid utf-8");
    buffer.drain(..index + delimiter_len);

    let mut output = String::new();
    let mut saw_stream_text = false;
    let terminal = process_sse_event(
        raw_event.as_str(),
        StreamResponseKind::Responses,
        &mut output,
        &mut saw_stream_text,
    )
    .expect("valid sse event");

    assert!(!terminal);
    assert_eq!(output, "你好世界");
    assert!(buffer.is_empty());
}

#[test]
fn byte_buffer_accepts_crlf_event_delimiter() {
    let event = b"data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\r\n\r\n";
    let (index, delimiter_len) =
        find_sse_event_delimiter(event.as_slice()).expect("crlf delimiter");
    let raw_event = decode_sse_event_bytes(event[..index].to_vec()).expect("valid utf-8");

    let mut output = String::new();
    let mut saw_stream_text = false;
    process_sse_event(
        raw_event.as_str(),
        StreamResponseKind::ChatCompletions,
        &mut output,
        &mut saw_stream_text,
    )
    .expect("valid sse event");

    assert_eq!(delimiter_len, 4);
    assert_eq!(output, "hello");
}
