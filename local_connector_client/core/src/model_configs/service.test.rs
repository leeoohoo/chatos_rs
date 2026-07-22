// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_configs::ModelConfigState;
    use crate::AuthState;

    fn authenticated_state_with_model(
        provider: &str,
        prompt_vendor: Option<&str>,
        base_url: &str,
    ) -> LocalState {
        LocalState {
            auth: Some(AuthState {
                cloud_base_url: "https://cloud.example.invalid".to_string(),
                user_service_base_url: "https://user.example.invalid".to_string(),
                access_token: "token".to_string(),
                device_name: "test-device".to_string(),
                user: Some(crate::AuthUserState {
                    id: "user-1".to_string(),
                    username: "user".to_string(),
                    display_name: "User".to_string(),
                    role: "user".to_string(),
                }),
            }),
            model_configs: ModelConfigState {
                configs: vec![LocalModelConfigRecord {
                    id: "model-1".to_string(),
                    server_model_config_id: None,
                    name: "Model".to_string(),
                    provider: provider.to_string(),
                    prompt_vendor: prompt_vendor.map(ToOwned::to_owned),
                    model: "test-model".to_string(),
                    base_url: Some(base_url.to_string()),
                    api_key: Some("secret".to_string()),
                    enabled: true,
                    supports_images: false,
                    supports_reasoning: true,
                    supports_responses: true,
                    thinking_level: None,
                    task_usage_scenario: None,
                    task_thinking_level: None,
                    temperature: None,
                    max_output_tokens: None,
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                    updated_at: "2026-01-01T00:00:00Z".to_string(),
                }],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn keeps_prompt_vendor_from_configured_provider_when_transport_is_compatible() {
        let state = authenticated_state_with_model(
            "gpt",
            None,
            "https://openai-compatible.example.invalid/v1",
        );

        let runtime =
            resolve_local_model_runtime(&state, "user-1", "model-1").expect("model runtime");

        assert_eq!(runtime.provider, "openai_compatible");
        assert_eq!(runtime.prompt_vendor.as_deref(), Some("gpt"));
    }

    #[test]
    fn removed_provider_values_cannot_be_resolved() {
        for provider in ["openai_compatible", "minimax"] {
            let state = authenticated_state_with_model(
                provider,
                None,
                "https://removed-provider.example.invalid/v1",
            );

            assert!(resolve_local_model_runtime(&state, "user-1", "model-1").is_err());
        }
    }

    #[test]
    fn glm_provider_uses_glm_prompt_over_the_compatible_transport() {
        let state =
            authenticated_state_with_model("glm", None, "https://open.bigmodel.cn/api/paas/v4");

        let runtime =
            resolve_local_model_runtime(&state, "user-1", "model-1").expect("model runtime");

        assert_eq!(runtime.provider, "openai_compatible");
        assert_eq!(runtime.prompt_vendor.as_deref(), Some("glm"));
    }

    #[test]
    fn deleting_environment_model_clears_environment_defaults() {
        let mut settings = LocalModelSettings {
            environment_initialization_model_config_id: Some("environment-model".to_string()),
            environment_initialization_thinking_level: Some("high".to_string()),
            ..Default::default()
        };

        settings.clear_model_id("environment-model");

        assert!(settings
            .environment_initialization_model_config_id
            .is_none());
        assert!(settings.environment_initialization_thinking_level.is_none());
    }

    #[test]
    fn local_model_retry_setting_defaults_to_five_and_validates_bounds() {
        assert_eq!(LocalModelSettings::default().model_request_max_retries, 5);

        let mut state = LocalState::default();
        let settings = LocalModelSettings {
            model_request_max_retries: 11,
            ..Default::default()
        };
        assert!(save_local_model_settings(&mut state, settings).is_err());
    }

    #[test]
    fn optional_text_update_can_clear_existing_value() {
        assert_eq!(optional_text_update(Some(""), Some("task planning")), None);
        assert_eq!(
            optional_text_update(None, Some("task planning")).as_deref(),
            Some("task planning")
        );
    }

    #[tokio::test]
    async fn remote_model_runtime_requests_never_return_device_credentials() {
        let response = handle_model_runtime_request(
            json!({
                "type": "model_runtime_request",
                "request_id": "request-1",
                "owner_user_id": "user-1",
                "device_id": "device-1",
                "workspace_id": "",
                "method": "GET",
                "path": "/model-runtime/model-1",
                "headers": {},
                "body": {"model_config_id": "model-1"}
            }),
            &LocalState::default(),
        )
        .await;
        assert_eq!(response.get("status").and_then(Value::as_u64), Some(403));
        assert_eq!(
            response
                .pointer("/body/error")
                .and_then(Value::as_str),
            Some(
                "Local model credentials are device-only; remote model runtime requests are disabled"
            )
        );
    }

    #[test]
    fn server_model_config_becomes_authoritative_local_copy() {
        let mut state =
            authenticated_state_with_model("gpt", Some("gpt"), "https://old.example.invalid/v1");
        state.model_configs.configs[0].server_model_config_id = Some("server-model-1".to_string());

        upsert_server_model_config(
            &mut state,
            &serde_json::json!({
                "id": "server-model-1",
                "name": "Managed model",
                "provider": "gpt",
                "prompt_vendor": "gpt",
                "model": "gpt-managed",
                "base_url": "https://managed.example.invalid/v1",
                "api_key": "server-secret",
                "enabled": true,
                "supports_images": true,
                "supports_reasoning": true,
                "supports_responses": true,
                "temperature": 0.3,
                "max_output_tokens": 4096,
                "created_at": "2026-07-01T00:00:00Z",
                "updated_at": "2026-07-17T00:00:00Z"
            }),
        )
        .expect("apply server model config");

        let local = &state.model_configs.configs[0];
        assert_eq!(local.id, "model-1");
        assert_eq!(
            local.server_model_config_id.as_deref(),
            Some("server-model-1")
        );
        assert_eq!(local.model, "gpt-managed");
        assert_eq!(local.api_key.as_deref(), Some("server-secret"));
        assert_eq!(local.max_output_tokens, Some(4096));
    }

    #[test]
    fn runtime_falls_back_from_legacy_model_without_credentials() {
        let mut state =
            authenticated_state_with_model("gpt", Some("gpt"), "https://old.example.invalid/v1");
        state.model_configs.configs[0].id = "legacy-model".to_string();
        state.model_configs.configs[0].server_model_config_id = Some("legacy-model".to_string());
        state.model_configs.configs[0].name = "my_api / gpt-5.4".to_string();
        state.model_configs.configs[0].model = "gpt-5.4".to_string();
        state.model_configs.configs[0].api_key = None;
        state.model_configs.configs[0].updated_at = "2026-07-10T00:00:00Z".to_string();
        let mut replacement = state.model_configs.configs[0].clone();
        replacement.id = "replacement-model".to_string();
        replacement.server_model_config_id = Some("replacement-model".to_string());
        replacement.base_url = Some("https://new.example.invalid/v1".to_string());
        replacement.api_key = Some("replacement-secret".to_string());
        replacement.updated_at = "2026-07-17T00:00:00Z".to_string();
        state.model_configs.configs.push(replacement);

        let runtime = resolve_local_model_runtime(&state, "user-1", "legacy-model")
            .expect("fallback model runtime");

        assert_eq!(runtime.local_model_config_id, "replacement-model");
        assert_eq!(runtime.id, "replacement-model");
        assert_eq!(runtime.api_key, "replacement-secret");
        assert_eq!(runtime.base_url, "https://new.example.invalid/v1");
    }

    #[test]
    fn reconciliation_rebinds_all_defaults_to_credentialed_replacement() {
        let mut state =
            authenticated_state_with_model("gpt", Some("gpt"), "https://old.example.invalid/v1");
        state.model_configs.configs[0].id = "legacy-model".to_string();
        state.model_configs.configs[0].name = "my_api / gpt-5.4".to_string();
        state.model_configs.configs[0].model = "gpt-5.4".to_string();
        state.model_configs.configs[0].api_key = None;
        let mut replacement = state.model_configs.configs[0].clone();
        replacement.id = "replacement-model".to_string();
        replacement.api_key = Some("replacement-secret".to_string());
        replacement.updated_at = "2026-07-17T00:00:00Z".to_string();
        state.model_configs.configs.push(replacement);
        state.model_configs.settings = LocalModelSettings {
            memory_summary_model_config_id: Some("legacy-model".to_string()),
            project_management_agent_model_config_id: Some("legacy-model".to_string()),
            environment_initialization_model_config_id: Some("legacy-model".to_string()),
            command_approval_model_config_id: Some("legacy-model".to_string()),
            ..Default::default()
        };

        assert_eq!(
            repair_model_settings_with_credential_fallbacks(&mut state),
            4
        );
        assert_eq!(
            state
                .model_configs
                .settings
                .memory_summary_model_config_id
                .as_deref(),
            Some("replacement-model")
        );
        assert_eq!(
            state
                .model_configs
                .settings
                .project_management_agent_model_config_id
                .as_deref(),
            Some("replacement-model")
        );
        assert_eq!(
            state
                .model_configs
                .settings
                .environment_initialization_model_config_id
                .as_deref(),
            Some("replacement-model")
        );
        assert_eq!(
            state
                .model_configs
                .settings
                .command_approval_model_config_id
                .as_deref(),
            Some("replacement-model")
        );
    }
}
