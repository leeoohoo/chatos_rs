// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::{ModelRequest, ModelRuntimeConfig};

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
    .with_request_body_limit_bytes(Some(2048))
    .with_max_transient_retries(Some(7));

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
    assert_eq!(request.max_transient_retries, Some(7));
    assert_eq!(request.tools.len(), 1);
}

#[test]
fn direct_vendor_runtime_configs_do_not_select_missing_responses_routes() {
    for (provider, base_url, model) in [
        ("kimi", "https://api.moonshot.ai/v1", "kimi-k2.6"),
        ("glm", "https://open.bigmodel.cn/api/paas/v4", "glm-5.2"),
    ] {
        let request = ModelRuntimeConfig::openai_compatible(base_url, "secret", model, provider)
            .with_responses_support(true)
            .to_model_request(json!("hello"), Vec::new());
        assert!(
            !request.supports_responses,
            "{provider} must use chat completions"
        );
    }

    let deepseek = ModelRuntimeConfig::openai_compatible(
        "https://api.deepseek.com",
        "secret",
        "deepseek-chat",
        "deepseek",
    )
    .with_responses_support(true)
    .to_model_request(json!("hello"), Vec::new());
    assert!(deepseek.supports_responses);
}
