use super::UserModelConfigRecord;

fn model(enabled: bool, task_enabled: Option<bool>) -> UserModelConfigRecord {
    UserModelConfigRecord {
        model_capability_revision: Some("capability-v1".to_string()),
        context_window_tokens: Some(200_000),
        output_reserve_tokens: Some(30_000),
        protocol_overhead_estimator_id: Some("responses-json-v1".to_string()),
        tokenizer_id: Some("o200k_base".to_string()),
        responses_compaction_threshold: Some(160_000),
        id: "model-1".to_string(),
        owner_user_id: "user-1".to_string(),
        source_provider_id: Some("provider-1".to_string()),
        name: "Model".to_string(),
        provider: "gpt".to_string(),
        prompt_vendor: Some("gpt".to_string()),
        model: "gpt-test".to_string(),
        thinking_level: None,
        task_usage_scenario: None,
        task_thinking_level: None,
        temperature: None,
        max_output_tokens: None,
        api_key: Some("secret".to_string()),
        has_api_key: true,
        base_url: Some("https://api.example.test/v1".to_string()),
        enabled,
        task_enabled,
        supports_images: false,
        supports_reasoning: false,
        supports_responses: true,
        created_at: "created".to_string(),
        updated_at: "updated".to_string(),
    }
}

#[test]
fn task_availability_is_independent_from_chat_availability() {
    assert!(model(true, Some(true)).enabled_for_tasks());
    assert!(!model(true, Some(false)).enabled_for_tasks());
    assert!(!model(false, Some(true)).enabled_for_tasks());
    assert!(model(true, None).enabled_for_tasks());
    assert!(!model(false, None).enabled_for_tasks());
}

#[test]
fn published_capability_view_is_never_serialized_into_model_storage() {
    let value = serde_json::to_value(model(true, Some(true))).unwrap();
    for key in [
        "model_capability_revision",
        "context_window_tokens",
        "output_reserve_tokens",
        "protocol_overhead_estimator_id",
        "tokenizer_id",
        "responses_compaction_threshold",
    ] {
        assert!(
            value.get(key).is_none(),
            "independent budget field survived: {key}"
        );
    }
    assert_eq!(value["id"], "model-1");
    assert_eq!(value["task_enabled"], true);
}

#[test]
fn model_update_cannot_write_an_independent_budget() {
    for key in [
        "context_window_tokens",
        "output_reserve_tokens",
        "model_capability_revision",
        "responses_compaction_threshold",
    ] {
        let value = serde_json::json!({key: "independent-value"});
        assert!(serde_json::from_value::<super::UpdateUserModelConfigRequest>(value).is_err());
    }
}
