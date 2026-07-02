// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{header::CONTENT_TYPE, Response};
use serde_json::{json, Value};
use tokio::time;

use super::super::parsing::{
    extract_chat_completion_stream_text, extract_responses_stream_text,
    extract_stream_error_message, trim_or_truncate_for_log,
};
use super::super::protocol::{
    base_url_disallows_system_messages, base_url_requires_responses_input_list,
    build_chat_completions_endpoint, build_chat_messages, build_responses_endpoint,
    build_responses_input,
};
use super::AiClient;

#[derive(Clone, Copy)]
enum StreamResponseKind {
    ChatCompletions,
    Responses,
}

pub(super) async fn send_text_request(
    client: &AiClient,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    requested_max_tokens: Option<i64>,
    effective_temperature: f64,
) -> Result<String, String> {
    if client.supports_responses {
        request_responses(
            client,
            api_key,
            system_prompt,
            user_prompt,
            requested_max_tokens,
            effective_temperature,
        )
        .await
    } else {
        request_chat_completions(
            client,
            api_key,
            system_prompt,
            user_prompt,
            requested_max_tokens,
            effective_temperature,
        )
        .await
    }
}

async fn request_chat_completions(
    client: &AiClient,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    requested_max_tokens: Option<i64>,
    effective_temperature: f64,
) -> Result<String, String> {
    let endpoint = build_chat_completions_endpoint(client.base_url.as_str());
    let mut body = json!({
        "model": client.model,
        "temperature": effective_temperature,
        "stream": true,
        "messages": build_chat_messages(
            system_prompt,
            user_prompt,
            base_url_disallows_system_messages(client.base_url.as_str()),
        )
    });
    if let Some(requested_max_tokens) = requested_max_tokens {
        body["max_tokens"] = json!(requested_max_tokens);
    }
    if client.disable_thinking {
        body["thinking"] = json!({ "type": "disabled" });
    }

    send_stream_request(
        client,
        api_key,
        endpoint.as_str(),
        &body,
        StreamResponseKind::ChatCompletions,
    )
    .await
}

async fn request_responses(
    client: &AiClient,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    requested_max_tokens: Option<i64>,
    effective_temperature: f64,
) -> Result<String, String> {
    let no_system_messages = base_url_disallows_system_messages(client.base_url.as_str());
    let wrapped_user_prompt = if no_system_messages && !system_prompt.trim().is_empty() {
        format!(
            "【系统上下文】\n{}\n\n{}",
            system_prompt.trim(),
            user_prompt
        )
    } else {
        user_prompt.to_string()
    };
    let input_as_list = base_url_requires_responses_input_list(client.base_url.as_str());
    let endpoint = build_responses_endpoint(client.base_url.as_str());
    let mut body = json!({
        "model": client.model,
        "temperature": effective_temperature,
        "stream": true,
        "input": build_responses_input(wrapped_user_prompt.as_str(), input_as_list),
    });
    if let Some(requested_max_tokens) = requested_max_tokens {
        body["max_output_tokens"] = json!(requested_max_tokens);
    }
    if !no_system_messages && !system_prompt.trim().is_empty() {
        body["instructions"] = Value::String(system_prompt.to_string());
    }
    if client.disable_thinking {
        body["thinking"] = json!({ "type": "disabled" });
    }

    send_stream_request(
        client,
        api_key,
        endpoint.as_str(),
        &body,
        StreamResponseKind::Responses,
    )
    .await
}

async fn send_stream_request(
    client: &AiClient,
    api_key: &str,
    endpoint: &str,
    body: &Value,
    response_kind: StreamResponseKind,
) -> Result<String, String> {
    let response = time::timeout(
        Duration::from_secs(client.timeout_secs),
        client
            .http
            .post(endpoint)
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(body)
            .send(),
    )
    .await
    .map_err(|_| {
        format!(
            "ai request timed out after {}s while waiting for response headers",
            client.timeout_secs
        )
    })?
    .map_err(|err| format!("ai request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let response_body = response.text().await.unwrap_or_default();
        return Err(format!(
            "ai request status={} endpoint={} body={}",
            status, endpoint, response_body
        ));
    }

    if !is_sse_response(&response) {
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown");
        return Err(format!(
            "ai stream expected content-type text/event-stream but got {}",
            content_type
        ));
    }

    read_streamed_text_response(client, response, response_kind).await
}

async fn read_streamed_text_response(
    client: &AiClient,
    response: Response,
    response_kind: StreamResponseKind,
) -> Result<String, String> {
    let mut stream = response.bytes_stream();
    let mut output = String::new();
    let mut saw_stream_text = false;
    let mut buffer = Vec::new();

    while let Some(next_chunk) =
        time::timeout(Duration::from_secs(client.timeout_secs), stream.next())
            .await
            .map_err(|_| {
                format!(
                    "ai request timed out after {}s while waiting for stream data",
                    client.timeout_secs
                )
            })?
    {
        let bytes = next_chunk.map_err(|err| format!("ai stream read failed: {err}"))?;
        buffer.extend_from_slice(&bytes);

        loop {
            let Some((index, delimiter_len)) = find_sse_event_delimiter(buffer.as_slice()) else {
                break;
            };
            let raw_event = decode_sse_event_bytes(buffer[..index].to_vec())?;
            buffer.drain(..index + delimiter_len);
            if process_sse_event(
                raw_event.as_str(),
                response_kind,
                &mut output,
                &mut saw_stream_text,
            )? {
                return finalize_stream_output(output);
            }
        }
    }

    if !bytes_trimmed_empty(buffer.as_slice()) {
        let raw_event = decode_sse_event_bytes(buffer)?;
        if process_sse_event(
            raw_event.as_str(),
            response_kind,
            &mut output,
            &mut saw_stream_text,
        )? {
            return finalize_stream_output(output);
        }
    }

    finalize_stream_output(output)
}

fn decode_sse_event_bytes(bytes: Vec<u8>) -> Result<String, String> {
    let mut event = String::from_utf8(bytes)
        .map_err(|err| format!("ai stream event utf-8 decode failed: {err}"))?;
    normalize_sse_newlines(&mut event);
    Ok(event)
}

fn find_sse_event_delimiter(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0;
    while index < buffer.len() {
        if index + 1 < buffer.len() {
            if buffer[index] == b'\n' && buffer[index + 1] == b'\n' {
                return Some((index, 2));
            }
            if buffer[index] == b'\r' && buffer[index + 1] == b'\r' {
                return Some((index, 2));
            }
        }
        if index + 3 < buffer.len() && &buffer[index..index + 4] == b"\r\n\r\n" {
            return Some((index, 4));
        }
        index += 1;
    }
    None
}

fn bytes_trimmed_empty(buffer: &[u8]) -> bool {
    buffer.iter().all(|byte| byte.is_ascii_whitespace())
}

fn process_sse_event(
    raw_event: &str,
    response_kind: StreamResponseKind,
    output: &mut String,
    saw_stream_text: &mut bool,
) -> Result<bool, String> {
    let mut payload_lines = Vec::new();
    for line in raw_event.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            payload_lines.push(rest.trim_start());
        }
    }

    if payload_lines.is_empty() {
        return Ok(false);
    }

    let payload = payload_lines.join("\n");
    if payload.trim() == "[DONE]" {
        return Ok(true);
    }

    let value: Value = serde_json::from_str(payload.as_str()).map_err(|err| {
        format!(
            "ai stream event decode failed: {} payload={}",
            err,
            trim_or_truncate_for_log(payload.as_str(), 800)
        )
    })?;

    if let Some(message) = extract_stream_error_message(&value) {
        return Err(format!("ai stream error: {}", message));
    }

    let text = match response_kind {
        StreamResponseKind::ChatCompletions => extract_chat_completion_stream_text(&value),
        StreamResponseKind::Responses => extract_responses_stream_text(&value, *saw_stream_text),
    };
    if let Some(text) = text {
        *saw_stream_text = true;
        output.push_str(text.as_str());
    }

    Ok(is_terminal_stream_event(&value))
}

fn finalize_stream_output(output: String) -> Result<String, String> {
    if output.trim().is_empty() {
        Err("ai empty content".to_string())
    } else {
        Ok(output)
    }
}

fn is_sse_response(response: &Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("text/event-stream"))
        .unwrap_or(false)
}

fn normalize_sse_newlines(buffer: &mut String) {
    if buffer.contains('\r') {
        *buffer = buffer.replace("\r\n", "\n").replace('\r', "\n");
    }
}

fn is_terminal_stream_event(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("response.completed") | Some("response.failed")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
