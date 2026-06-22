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
    let mut buffer = String::new();

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
        buffer.push_str(String::from_utf8_lossy(&bytes).as_ref());
        normalize_sse_newlines(&mut buffer);

        loop {
            let Some(index) = buffer.find("\n\n") else {
                break;
            };
            let raw_event = buffer[..index].to_string();
            buffer.drain(..index + 2);
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

    normalize_sse_newlines(&mut buffer);
    if !buffer.trim().is_empty()
        && process_sse_event(
            buffer.as_str(),
            response_kind,
            &mut output,
            &mut saw_stream_text,
        )?
    {
        return finalize_stream_output(output);
    }

    finalize_stream_output(output)
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
