use super::UserModelConfigRecord;

fn model(enabled: bool, task_enabled: Option<bool>) -> UserModelConfigRecord {
    UserModelConfigRecord {
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
