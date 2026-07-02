// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use serde::Serialize;

use crate::models::{SkillInstallStatus, SkillScope};

use super::*;

#[derive(Debug, Serialize)]
pub(super) struct InternalExecutionOptionsResponse {
    pub model_config_ids: Vec<String>,
    pub builtin_tool_ids: Vec<String>,
    pub external_tool_ids: Vec<String>,
    pub skill_ids: Vec<String>,
}

pub(super) async fn get_user_execution_options(
    Path(owner_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<InternalExecutionOptionsResponse>, ApiError> {
    require_internal_api_secret(&state, &headers)?;
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err(ApiError::bad_request("owner_user_id is required"));
    }

    let model_config_ids = state
        .model_config_service
        .list_model_configs()
        .await
        .map_err(ApiError::bad_request)?
        .into_iter()
        .filter(|model| model.enabled)
        .filter(|model| owns_resource(model.owner_user_id.as_deref(), owner_user_id))
        .map(|model| model.id)
        .collect::<BTreeSet<_>>();

    let mut builtin_tool_ids = BTreeSet::new();
    for item in state.mcp_catalog_service.list_catalog() {
        builtin_tool_ids.insert(item.kind);
        if let Some(config_id) = item.config_id {
            builtin_tool_ids.insert(config_id);
        }
    }

    let external_tool_ids = state
        .external_mcp_config_service
        .list_external_mcp_configs()
        .await
        .map_err(ApiError::bad_request)?
        .into_iter()
        .filter(|config| config.enabled)
        .filter(|config| {
            owns_resource(
                resource_owner_or_creator(
                    config.owner_user_id.as_deref(),
                    config.creator_user_id.as_deref(),
                ),
                owner_user_id,
            )
        })
        .map(|config| config.id)
        .collect::<BTreeSet<_>>();

    let skill_ids = state
        .skill_service
        .list_skills(SkillListFilters::default())
        .await
        .map_err(ApiError::bad_request)?
        .into_iter()
        .filter(|skill| skill.enabled && skill.install_status == SkillInstallStatus::Installed)
        .filter(|skill| {
            skill.scope == SkillScope::AdminGlobal
                || owns_resource(
                    resource_owner_or_creator(
                        skill.owner_user_id.as_deref(),
                        skill.creator_user_id.as_deref(),
                    ),
                    owner_user_id,
                )
        })
        .map(|skill| skill.id)
        .collect::<BTreeSet<_>>();

    Ok(Json(InternalExecutionOptionsResponse {
        model_config_ids: model_config_ids.into_iter().collect(),
        builtin_tool_ids: builtin_tool_ids.into_iter().collect(),
        external_tool_ids: external_tool_ids.into_iter().collect(),
        skill_ids: skill_ids.into_iter().collect(),
    }))
}

fn require_internal_api_secret(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state
        .config
        .internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(ApiError::forbidden(
            "task runner internal api secret is not configured",
        ));
    };
    let provided = headers
        .get("x-task-runner-internal-secret")
        .or_else(|| headers.get("x-project-service-sync-secret"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("missing task runner internal api secret"))?;
    if provided != expected {
        return Err(ApiError::unauthorized(
            "invalid task runner internal api secret",
        ));
    }
    Ok(())
}

fn owns_resource(owner_user_id: Option<&str>, expected_owner_user_id: &str) -> bool {
    owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(expected_owner_user_id)
}

fn resource_owner_or_creator<'a>(
    owner_user_id: Option<&'a str>,
    creator_user_id: Option<&'a str>,
) -> Option<&'a str> {
    owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            creator_user_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::HeaderValue;

    use super::*;
    use crate::ask_user_prompt_service::AskUserPromptService;
    use crate::auth::AuthService;
    use crate::config::{AppConfig, StoreMode};
    use crate::mcp_server::TaskRunnerMcpService;
    use crate::models::{ExternalMcpConfigRecord, ModelConfigRecord};
    use crate::services::{
        ExternalMcpConfigService, McpCatalogService, ModelConfigService, RemoteServerService,
        RunService, SkillService, TaskProjectService, TaskService, ToolingStateService,
    };
    use crate::store::AppStore;

    #[tokio::test]
    async fn user_execution_options_filters_owner_scoped_configs() {
        let state = test_state().await;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-internal-secret",
            HeaderValue::from_static("internal-secret"),
        );

        let Json(response) =
            get_user_execution_options(Path("owner-1".to_string()), State(state), headers)
                .await
                .expect("execution options");

        assert_eq!(response.model_config_ids, vec!["model-owner"]);
        assert!(response.builtin_tool_ids.iter().any(|id| !id.is_empty()));
        assert_eq!(
            response.external_tool_ids,
            vec!["external-created-by-owner", "external-owner"]
        );
    }

    #[tokio::test]
    async fn user_execution_options_requires_internal_secret() {
        let state = test_state().await;

        let err =
            get_user_execution_options(Path("owner-1".to_string()), State(state), HeaderMap::new())
                .await
                .expect_err("missing secret should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "missing task runner internal api secret");
    }

    async fn test_state() -> AppState {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        store
            .save_model_config(model_config("model-owner", Some("owner-1"), true))
            .await
            .expect("save owner model");
        store
            .save_model_config(model_config("model-other", Some("owner-2"), true))
            .await
            .expect("save other model");
        store
            .save_model_config(model_config("model-disabled", Some("owner-1"), false))
            .await
            .expect("save disabled model");
        store
            .save_external_mcp_config(external_config(
                "external-owner",
                Some("owner-1"),
                None,
                true,
            ))
            .await
            .expect("save owner external mcp");
        store
            .save_external_mcp_config(external_config(
                "external-created-by-owner",
                None,
                Some("owner-1"),
                true,
            ))
            .await
            .expect("save creator external mcp");
        store
            .save_external_mcp_config(external_config(
                "external-other",
                Some("owner-2"),
                None,
                true,
            ))
            .await
            .expect("save other external mcp");
        store
            .save_external_mcp_config(external_config(
                "external-disabled",
                Some("owner-1"),
                None,
                false,
            ))
            .await
            .expect("save disabled external mcp");

        let auth_service = AuthService::new(config.clone(), store.clone());
        let task_service = TaskService::new(config.clone(), store.clone());
        let model_config_service = ModelConfigService::new(store.clone());
        let remote_server_service = RemoteServerService::new(store.clone());
        let external_mcp_config_service = ExternalMcpConfigService::new(store.clone());
        let skill_service = SkillService::new(&config, store.clone());
        let task_project_service = TaskProjectService::new(store.clone());
        let ask_user_prompt_service = AskUserPromptService::new(store.clone());
        let run_service = RunService::new(
            config.clone(),
            store.clone(),
            ask_user_prompt_service.clone(),
        );
        let mcp_catalog_service =
            McpCatalogService::new(task_service.clone(), ask_user_prompt_service.clone());
        let tooling_state_service = ToolingStateService::new(config.clone());
        let task_runner_mcp_service = TaskRunnerMcpService::new(
            task_service.clone(),
            model_config_service.clone(),
            external_mcp_config_service.clone(),
            skill_service.clone(),
            run_service.clone(),
            ask_user_prompt_service.clone(),
            mcp_catalog_service.clone(),
        );

        AppState {
            config,
            task_service,
            model_config_service,
            remote_server_service,
            external_mcp_config_service,
            skill_service,
            task_project_service,
            run_service,
            ask_user_prompt_service,
            mcp_catalog_service,
            tooling_state_service,
            task_runner_mcp_service,
            auth_service,
        }
    }

    fn test_config() -> AppConfig {
        let default_workspace_dir = std::env::temp_dir()
            .join("chatos-task-runner-internal-options-test")
            .to_string_lossy()
            .into_owned();
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://internal-execution-options-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir,
            memory_timeout: Duration::from_millis(1_000),
            execution_timeout: Duration::from_millis(1_000),
            scheduler_poll_interval: Duration::from_millis(1_000),
            worker_id: "test-worker".to_string(),
            worker_poll_interval: Duration::from_millis(1_000),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 2_000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: Some("internal-secret".to_string()),
            callback_timeout: Duration::from_millis(1_000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5_000),
            project_service_base_url: None,
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(5_000),
        }
    }

    fn model_config(id: &str, owner_user_id: Option<&str>, enabled: bool) -> ModelConfigRecord {
        ModelConfigRecord {
            id: id.to_string(),
            owner_user_id: owner_user_id.map(ToOwned::to_owned),
            owner_username: None,
            owner_display_name: None,
            name: id.to_string(),
            provider: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "gpt-test".to_string(),
            usage_scenario: None,
            temperature: None,
            max_output_tokens: None,
            thinking_level: None,
            supports_responses: true,
            instructions: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            enabled,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn external_config(
        id: &str,
        owner_user_id: Option<&str>,
        creator_user_id: Option<&str>,
        enabled: bool,
    ) -> ExternalMcpConfigRecord {
        ExternalMcpConfigRecord {
            id: id.to_string(),
            name: id.to_string(),
            transport: "stdio".to_string(),
            command: Some("echo".to_string()),
            args: vec!["ok".to_string()],
            url: None,
            headers: BTreeMap::new(),
            env: BTreeMap::new(),
            cwd: None,
            enabled,
            creator_user_id: creator_user_id.map(ToOwned::to_owned),
            creator_username: None,
            creator_display_name: None,
            owner_user_id: owner_user_id.map(ToOwned::to_owned),
            owner_username: None,
            owner_display_name: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}
