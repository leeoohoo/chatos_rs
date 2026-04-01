use axum::http::StatusCode;
use reqwest::Client;
use serde_json::{json, Value};

use super::{
    bad_gateway_error,
    stream_support::{
        adapt_responses_to_chat_completion, aggregate_chat_completions_stream,
        aggregate_responses_stream, build_responses_input_from_messages, read_sse_json_events,
    },
    support::{
        build_chat_completion_endpoint, build_responses_endpoint, format_transport_error,
        request_timeout_for_runtime,
    },
    ModelRuntime,
};

pub(super) async fn request_chat_completion(
    http: &Client,
    runtime: &ModelRuntime,
    messages: &[Value],
    tools: Option<&[Value]>,
) -> Result<Value, (StatusCode, String)> {
    if runtime.supports_responses {
        return request_responses_completion(http, runtime, messages, tools).await;
    }

    request_chat_completions(http, runtime, messages, tools).await
}

pub(super) async fn request_chat_completions(
    http: &Client,
    runtime: &ModelRuntime,
    messages: &[Value],
    tools: Option<&[Value]>,
) -> Result<Value, (StatusCode, String)> {
    let mut body = json!({
        "model": runtime.model,
        "temperature": runtime.temperature,
        "max_tokens": 2400,
        "stream": true,
        "stream_options": {"include_usage": true},
        "messages": messages,
    });

    if let Some(tool_items) = tools {
        body["tools"] = Value::Array(tool_items.to_vec());
        body["tool_choice"] = Value::String("auto".to_string());
    }

    let endpoint = build_chat_completion_endpoint(runtime.base_url.as_str());
    let mut request = http
        .post(endpoint.as_str())
        .bearer_auth(runtime.api_key.as_str())
        .header("Content-Type", "application/json")
        .header("Connection", "close")
        .json(&body);
    if let Some(timeout) = request_timeout_for_runtime(runtime) {
        request = request.timeout(timeout);
    }
    let response = request.send().await.map_err(|err| {
        bad_gateway_error(format_transport_error(runtime, endpoint.as_str(), &err))
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let payload = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "agent builder ai request status={} provider={} model={} endpoint={} body={}",
                status, runtime.provider, runtime.model, endpoint, payload
            ),
        ));
    }

    let events = read_sse_json_events(response).await?;
    aggregate_chat_completions_stream(events.as_slice())
}

pub(super) async fn request_responses_completion(
    http: &Client,
    runtime: &ModelRuntime,
    messages: &[Value],
    tools: Option<&[Value]>,
) -> Result<Value, (StatusCode, String)> {
    let mut body = json!({
        "model": runtime.model,
        "temperature": runtime.temperature,
        "max_output_tokens": 2400,
        "stream": true,
        "input": build_responses_input_from_messages(messages),
    });

    if let Some(tool_items) = tools {
        body["tools"] = Value::Array(tool_items.to_vec());
        body["tool_choice"] = Value::String("auto".to_string());
    }

    let endpoint = build_responses_endpoint(runtime.base_url.as_str());
    let mut request = http
        .post(endpoint.as_str())
        .bearer_auth(runtime.api_key.as_str())
        .header("Content-Type", "application/json")
        .header("Connection", "close")
        .json(&body);
    if let Some(timeout) = request_timeout_for_runtime(runtime) {
        request = request.timeout(timeout);
    }
    let response = request.send().await.map_err(|err| {
        bad_gateway_error(format_transport_error(runtime, endpoint.as_str(), &err))
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let payload = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "agent builder ai request status={} provider={} model={} endpoint={} body={}",
                status, runtime.provider, runtime.model, endpoint, payload
            ),
        ));
    }

    let events = read_sse_json_events(response).await?;
    let payload = aggregate_responses_stream(events.as_slice())?;

    Ok(adapt_responses_to_chat_completion(payload))
}
