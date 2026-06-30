use serde_json::json;

use super::{
    ModelRequest, ModelRuntimeConfig, RuntimeRecordOptions, SaveAssistantRecordInput,
    SaveRecordInput,
};

#[test]
fn model_runtime_config_builds_model_request() {
    let config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:8080/v1",
        "secret",
        "gpt-test",
        "openai",
    )
    .with_responses_support(true)
    .with_instructions(Some("system prompt".to_string()))
    .with_temperature(Some(0.2))
    .with_max_output_tokens(Some(1024))
    .with_thinking_level(Some("medium".to_string()))
    .with_prompt_cache_key(Some("task-1".to_string()))
    .with_request_cwd(Some("/tmp/work".to_string()))
    .with_prompt_cache_retention(true)
    .with_request_body_limit_bytes(Some(2048));

    let request =
        ModelRequest::from_runtime_config(&config, json!("hello"), vec![json!({"name":"t"})]);

    assert_eq!(request.base_url, "http://127.0.0.1:8080/v1");
    assert_eq!(request.api_key, "secret");
    assert_eq!(request.model, "gpt-test");
    assert_eq!(request.provider, "openai");
    assert!(request.supports_responses);
    assert_eq!(request.instructions.as_deref(), Some("system prompt"));
    assert_eq!(request.temperature, Some(0.2));
    assert_eq!(request.max_output_tokens, Some(1024));
    assert_eq!(request.thinking_level.as_deref(), Some("medium"));
    assert_eq!(request.prompt_cache_key.as_deref(), Some("task-1"));
    assert_eq!(request.request_cwd.as_deref(), Some("/tmp/work"));
    assert!(request.include_prompt_cache_retention);
    assert_eq!(request.request_body_limit_bytes, Some(2048));
    assert_eq!(request.tools.len(), 1);
}

#[test]
fn save_record_input_builders_pack_runtime_metadata() {
    let input = SaveRecordInput::user_message("task_1", "hello")
        .with_conversation_turn_id("run_1")
        .with_message_id("message_1")
        .with_message_mode("task")
        .with_message_source("task_runner")
        .with_metadata(json!({"task_id": "task_1"}));

    assert_eq!(input.role, "user");
    assert_eq!(input.content, "hello");
    assert_eq!(input.message_id.as_deref(), Some("message_1"));

    let metadata = input.packed_metadata().expect("metadata");
    assert_eq!(metadata["task_id"].as_str(), Some("task_1"));
    assert_eq!(metadata["conversation_turn_id"].as_str(), Some("run_1"));
    assert_eq!(metadata["message_mode"].as_str(), Some("task"));
    assert_eq!(metadata["message_source"].as_str(), Some("task_runner"));
}

#[test]
fn runtime_record_options_builders_configure_persistence() {
    let options = RuntimeRecordOptions::default()
        .with_persist_assistant_records(true)
        .with_persist_tool_records(true)
        .with_assistant_message_mode("task_assistant")
        .with_assistant_message_source("task_runner")
        .with_assistant_metadata(json!({"kind": "assistant"}))
        .with_tool_message_mode("task_tool")
        .with_tool_message_source("task_runner")
        .with_tool_metadata(json!({"kind": "tool"}));

    assert!(options.persist_assistant_records);
    assert!(options.persist_tool_records);
    assert_eq!(
        options.assistant_message_mode.as_deref(),
        Some("task_assistant")
    );
    assert_eq!(options.tool_message_mode.as_deref(), Some("task_tool"));
    assert_eq!(
        options
            .assistant_metadata
            .as_ref()
            .and_then(|v| v["kind"].as_str()),
        Some("assistant")
    );
    assert_eq!(
        options
            .tool_metadata
            .as_ref()
            .and_then(|v| v["kind"].as_str()),
        Some("tool")
    );
}

#[test]
fn assistant_record_input_preserves_structured_payload_and_tool_calls() {
    let tool_calls = json!([{
        "id": "call_1",
        "type": "function",
        "function": {
            "name": "demo.search",
            "arguments": "{\"q\":\"rust\"}"
        }
    }]);
    let record: SaveRecordInput = SaveAssistantRecordInput {
        conversation_id: "task_1".to_string(),
        conversation_turn_id: Some("run_1".to_string()),
        message_id: Some("message_1".to_string()),
        content: "calling tool".to_string(),
        reasoning: Some("need data".to_string()),
        structured_payload: Some(tool_calls.clone()),
        metadata: Some(json!({"task_id": "task_1"})),
        tool_calls: Some(tool_calls.clone()),
        response_id: Some("resp_1".to_string()),
        response_status: Some("tool_calls".to_string()),
        message_mode: Some("task_run".to_string()),
        message_source: Some("task_runner".to_string()),
        summary_status: None,
        summary_id: None,
        summarized_at: None,
        created_at: None,
    }
    .into();

    assert_eq!(record.role, "assistant");
    assert_eq!(record.structured_payload, Some(tool_calls.clone()));
    assert_eq!(record.tool_calls, Some(tool_calls));
    let metadata = record.packed_metadata().expect("metadata");
    assert_eq!(metadata["response_status"].as_str(), Some("tool_calls"));
}
