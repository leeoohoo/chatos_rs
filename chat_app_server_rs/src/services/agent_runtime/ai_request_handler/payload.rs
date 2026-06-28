use chatos_ai_runtime::request_payload::{
    build_chat_completions_request_payload as build_shared_chat_completions_request_payload,
    build_responses_request_payload as build_shared_responses_request_payload,
};
use serde_json::Value;

#[cfg(test)]
pub(super) fn build_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    prompt_cache_key: Option<String>,
    tools: Option<Vec<Value>>,
    request_cwd: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    stream: bool,
    include_prompt_cache_retention: bool,
) -> Value {
    build_responses_request_payload(
        input,
        model,
        instructions,
        prompt_cache_key,
        tools,
        request_cwd,
        temperature,
        max_output_tokens,
        provider,
        thinking_level,
        stream,
        include_prompt_cache_retention,
    )
}

pub(super) fn build_responses_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    prompt_cache_key: Option<String>,
    tools: Option<Vec<Value>>,
    request_cwd: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    stream: bool,
    include_prompt_cache_retention: bool,
) -> Value {
    build_shared_responses_request_payload(
        input,
        model,
        instructions,
        prompt_cache_key,
        tools,
        request_cwd,
        temperature,
        max_output_tokens,
        provider,
        thinking_level,
        stream,
        include_prompt_cache_retention,
    )
}

pub(super) fn build_chat_completions_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    tools: Option<Vec<Value>>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    stream: bool,
) -> Value {
    build_shared_chat_completions_request_payload(
        input,
        model,
        instructions,
        tools,
        temperature,
        max_output_tokens,
        provider,
        thinking_level,
        stream,
    )
}
