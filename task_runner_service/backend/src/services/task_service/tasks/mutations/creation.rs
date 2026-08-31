// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::normalize_task_profile;
use crate::models::TaskPluginSelectionAudit;
use crate::services::status_display::TaskScheduleModeExt;

impl TaskService {
    pub async fn create_task(
        &self,
        input: CreateTaskRequest,
        creator: Option<&CurrentUser>,
        source_context: Option<TaskSourceContext>,
    ) -> Result<TaskRecord, String> {
        self.create_task_with_plugin_selection_audit(input, creator, source_context, None)
            .await
    }

    pub(crate) async fn create_task_with_plugin_selection_audit(
        &self,
        input: CreateTaskRequest,
        creator: Option<&CurrentUser>,
        source_context: Option<TaskSourceContext>,
        plugin_selection_audit: Option<TaskPluginSelectionAudit>,
    ) -> Result<TaskRecord, String> {
        if (!input.plugin_config.selected_plugins.is_empty()
            || !input.plugin_config.command_invocations.is_empty())
            && plugin_selection_audit.is_none()
        {
            return Err(
                "Task Plugin selection must be produced by the trusted Task Runner selector"
                    .to_string(),
            );
        }
        validate_required("title", &input.title)?;
        validate_required("objective", &input.objective)?;
        if let Some(model_config_id) = input.default_model_config_id.as_deref() {
            self.ensure_model_config_access(model_config_id, creator)
                .await?;
        }
        let task_owner_user_id =
            creator.and_then(|user| user.effective_owner_user_id().map(ToOwned::to_owned));
        let task_owner_username =
            creator.and_then(|user| user.effective_owner_username().map(ToOwned::to_owned));
        let task_owner_display_name = creator.and_then(|user| {
            user.effective_owner_display_name()
                .map(ToOwned::to_owned)
                .or_else(|| user.effective_owner_username().map(ToOwned::to_owned))
        });
        if matches!(input.status, Some(TaskStatus::Queued | TaskStatus::Running)) {
            return Err(
                "任务排队/运行状态由系统维护，请通过执行任务进入 queued 或 running".to_string(),
            );
        }
        let prerequisite_task_ids = normalize_prerequisite_task_ids(
            input.prerequisite_task_ids.clone().unwrap_or_default(),
        );

        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let mut source_context = source_context.unwrap_or_default();
        if source_context.project_id.is_none() {
            source_context.project_id = input.project_id.clone();
        }
        let project_id = normalize_project_id(source_context.project_id.clone());
        if project_id != PUBLIC_PROJECT_ID {
            self.ensure_project_available_for_task(&project_id, creator)
                .await?;
        }
        let task_profile = normalize_task_profile(input.task_profile.as_deref())?;
        self.validate_task_prerequisites_for_project(
            &id,
            &prerequisite_task_ids,
            creator,
            Some(project_id.as_str()),
        )
        .await?;
        let schedule = sanitize_task_schedule_config(input.schedule.unwrap_or_default(), None)?;
        let plugin_config = input.plugin_config;
        let requested_mcp_config = input.mcp_config.unwrap_or_default();
        let mut mcp_config = TaskMcpConfig::default();
        if let Some(requires_execution) = requested_mcp_config.requires_execution {
            mcp_config.requires_execution = requires_execution;
        }
        if let Some(workspace_changes_required) = requested_mcp_config.workspace_changes_required {
            mcp_config.workspace_changes_required = workspace_changes_required;
        }
        mcp_config.enabled_builtin_kinds = requested_mcp_config.enabled_builtin_kinds;
        mcp_config.external_mcp_config_ids = requested_mcp_config.external_mcp_config_ids;
        if let Some(builtin_prompt_locale) =
            normalized_optional(source_context.builtin_prompt_locale.clone())
        {
            mcp_config.builtin_prompt_locale = builtin_prompt_locale;
        }
        let mut mcp_config = sanitize_task_mcp_config(mcp_config);
        let agent_key = crate::models::task_runner_agent_key_for(
            task_profile.as_str(),
            mcp_config.requires_execution,
        );
        let input_payload = input.input_payload;
        if let Some(workspace_dir) = normalized_optional(source_context.workspace_dir.clone()) {
            mcp_config.workspace_dir = Some(workspace_dir);
        }
        if mcp_config.workspace_dir.is_some() {
            let _ = ensure_workspace_dir_available(
                self.config.default_workspace_dir.as_str(),
                mcp_config.workspace_dir.as_deref(),
            )?;
        }
        let capability_policy_resolved = self
            .validate_task_mcp_config_for_agent(
                &mcp_config,
                &plugin_config,
                project_id.as_str(),
                creator,
                task_owner_user_id.as_deref(),
                agent_key,
                task_profile.as_str(),
                schedule.mode.mode_key(),
            )
            .await?;
        let allow_unresolved_plugin_policy = {
            #[cfg(test)]
            {
                self.allow_unresolved_plugin_policy_for_test
            }
            #[cfg(not(test))]
            {
                false
            }
        };
        if !capability_policy_resolved
            && !allow_unresolved_plugin_policy
            && (!mcp_config.enabled_builtin_kinds.is_empty()
                || !mcp_config.external_mcp_config_ids.is_empty()
                || !plugin_config.selected_plugins.is_empty())
        {
            return Err(
                "Plugin Management policy is required to validate task MCP and Plugin selection"
                    .to_string(),
            );
        }
        if let Some(policy) = self
            .resolve_task_runner_policy_for_agent_project(
                creator,
                task_owner_user_id.as_deref(),
                agent_key,
                project_id.as_str(),
                Some(task_profile.as_str()),
                Some(schedule.mode.mode_key()),
            )
            .await?
        {
            mcp_config.skill_policy_revision = Some(policy.policy_revision().to_string());
        }
        let tenant_id = resolve_task_tenant_id(
            input.tenant_id,
            creator,
            self.config.default_tenant_id.as_str(),
        )?;
        let task = TaskRecord {
            id: id.clone(),
            title: input.title.trim().to_string(),
            description: normalized_optional(input.description),
            objective: input.objective.trim().to_string(),
            input_payload,
            status: input.status.unwrap_or(TaskStatus::Draft),
            priority: input.priority.unwrap_or(0),
            tags: normalize_tags(input.tags),
            default_model_config_id: normalized_optional(input.default_model_config_id),
            memory_thread_id: format!("task-{id}"),
            tenant_id,
            subject_id: input
                .subject_id
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| self.config.default_subject_id.clone()),
            project_id,
            task_profile,
            creator_user_id: creator.map(|user| user.id.clone()),
            creator_username: creator.map(|user| user.username.clone()),
            creator_display_name: creator.map(|user| user.display_name.clone()),
            owner_user_id: task_owner_user_id,
            owner_username: task_owner_username,
            owner_display_name: task_owner_display_name,
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule,
            parent_task_id: None,
            source_run_id: None,
            source_session_id: normalized_optional(source_context.source_session_id),
            source_turn_id: normalized_optional(source_context.source_turn_id),
            source_user_message_id: normalized_optional(source_context.source_user_message_id),
            prerequisite_task_ids: prerequisite_task_ids.clone(),
            task_tool_state: TaskToolState::default(),
            plugin_config,
            plugin_selection_audit,
            mcp_config,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        info!(
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            builtin_mcp_kinds = %task.mcp_config.enabled_builtin_kinds.join(","),
            external_mcp_config_ids = %task.mcp_config.external_mcp_config_ids.join(","),
            selected_skill_ids = %task.mcp_config.selected_skill_ids.join(","),
            selected_plugin_ids = %task.plugin_config.selected_plugins.iter().map(|plugin| plugin.plugin_id.as_str()).collect::<Vec<_>>().join(","),
            "task runner created task with MCP, Skill, and Plugin selection"
        );
        self.ensure_task_thread(&task).await?;
        let saved = self.store.save_task(task).await?;
        self.store
            .set_task_prerequisites(&id, prerequisite_task_ids)
            .await?;
        let hydrated = self.hydrate_task_prerequisites(saved).await?;
        Ok(hydrated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, StoreMode};
    use crate::models::UserRole;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://task-create-project-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            memory_engine_http_client: reqwest::Client::new(),
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(1000),
            execution_timeout: Duration::from_millis(1000),
            scheduler_poll_interval: Duration::from_millis(1000),
            worker_id: "test-worker".to_string(),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1000,
            default_tool_results_model_total_max_chars: 2000,
            chatos_callback_url: String::new(),
            chatos_callback_http_client: reqwest::Client::new(),
            internal_api_secret: None,
            chatos_internal_api_secret: None,
            mcp_management_internal_api_secret: None,
            user_service_internal_api_secret: None,
            callback_timeout: Duration::from_millis(1000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5000),
            project_service_base_url: None,
            project_service_internal_base_url: None,
            project_service_internal_http_client: reqwest::Client::new(),
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(5000),
        }
    }

    async fn test_service() -> TaskService {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        TaskService::new(config, store)
    }

    fn agent_user(owner_user_id: &str) -> CurrentUser {
        CurrentUser {
            id: format!("agent-{owner_user_id}"),
            username: format!("agent-{owner_user_id}"),
            display_name: format!("Agent {owner_user_id}"),
            role: UserRole::Agent,
            owner_user_id: Some(owner_user_id.to_string()),
            owner_username: Some(format!("user-{owner_user_id}")),
            owner_display_name: Some(format!("User {owner_user_id}")),
        }
    }

    async fn save_project(
        service: &TaskService,
        id: &str,
        owner_user_id: &str,
        status: TaskProjectStatus,
    ) -> TaskProjectRecord {
        let now = now_rfc3339();
        service
            .store
            .save_task_project(TaskProjectRecord {
                id: id.to_string(),
                owner_user_id: Some(owner_user_id.to_string()),
                owner_username: Some(format!("user-{owner_user_id}")),
                owner_display_name: Some(format!("User {owner_user_id}")),
                name: format!("Project {id}"),
                root_path: Some(format!("/workspace/{id}")),
                git_url: Some(format!("https://example.com/{id}.git")),
                cloud_import_source: None,
                import_status: None,
                source_git_url: None,
                harness_repo_identifier: None,
                harness_git_url: None,
                harness_default_branch: None,
                description: None,
                status,
                created_at: now.clone(),
                updated_at: now.clone(),
                archived_at: (status == TaskProjectStatus::Archived).then_some(now),
            })
            .await
            .expect("save project")
    }

    fn create_task_request(title: &str) -> CreateTaskRequest {
        CreateTaskRequest {
            title: title.to_string(),
            description: None,
            objective: format!("do {title}"),
            input_payload: None,
            status: None,
            priority: None,
            tags: None,
            default_model_config_id: None,
            project_id: None,
            task_profile: None,
            tenant_id: None,
            subject_id: None,
            schedule: None,
            plugin_config: Default::default(),
            mcp_config: None,
            prerequisite_task_ids: None,
        }
    }

    async fn create_task_with_project(
        service: &TaskService,
        project_id: Option<&str>,
        creator: Option<&CurrentUser>,
    ) -> Result<TaskRecord, String> {
        service
            .create_task(
                create_task_request("project task"),
                creator,
                Some(TaskSourceContext {
                    project_id: project_id.map(ToOwned::to_owned),
                    ..TaskSourceContext::default()
                }),
            )
            .await
    }

    #[tokio::test]
    async fn create_task_persists_source_project_id() {
        let service = test_service().await;
        let creator = agent_user("owner-a");
        let project =
            save_project(&service, "project-a", "owner-a", TaskProjectStatus::Active).await;

        let task = create_task_with_project(&service, Some(" project-a "), Some(&creator))
            .await
            .expect("create task");

        assert_eq!(task.project_id, project.id);
        let stored = service
            .get_task(task.id.as_str())
            .await
            .expect("get task")
            .expect("task");
        assert_eq!(stored.project_id, project.id);
    }

    #[tokio::test]
    async fn create_task_defaults_legacy_zero_and_empty_project_to_public() {
        let service = test_service().await;

        let task_without_context = service
            .create_task(create_task_request("public task"), None, None)
            .await
            .expect("create task without context");
        assert_eq!(task_without_context.project_id, PUBLIC_PROJECT_ID);

        let task_with_zero = create_task_with_project(&service, Some("0"), None)
            .await
            .expect("create task with legacy zero");
        assert_eq!(task_with_zero.project_id, PUBLIC_PROJECT_ID);

        let task_with_empty = create_task_with_project(&service, Some("   "), None)
            .await
            .expect("create task with empty project");
        assert_eq!(task_with_empty.project_id, PUBLIC_PROJECT_ID);
    }

    #[tokio::test]
    async fn create_task_rejects_untrusted_plugin_config() {
        let service = test_service().await;
        let mut request = create_task_request("Plugin task");
        request.plugin_config.selected_plugins.push(
            chatos_plugin_management_sdk::SelectedPluginRef {
                plugin_id: "plugin-browser".to_string(),
                selected_skill_ids: Vec::new(),
                selected_command_ids: Vec::new(),
                selected_agent_ids: Vec::new(),
            },
        );

        let error = service
            .create_task(request, None, None)
            .await
            .expect_err("untrusted Plugin config must fail closed");

        assert!(error.contains("trusted Task Runner selector"));
    }

    #[tokio::test]
    async fn create_task_rejects_missing_project() {
        let service = test_service().await;
        let creator = agent_user("owner-a");

        let err = create_task_with_project(&service, Some("missing-project"), Some(&creator))
            .await
            .expect_err("missing project should be rejected");

        assert!(err.contains("项目不存在"));
    }

    #[tokio::test]
    async fn create_task_rejects_archived_project() {
        let service = test_service().await;
        let creator = agent_user("owner-a");
        save_project(
            &service,
            "archived-project",
            "owner-a",
            TaskProjectStatus::Archived,
        )
        .await;

        let err = create_task_with_project(&service, Some("archived-project"), Some(&creator))
            .await
            .expect_err("archived project should be rejected");

        assert!(err.contains("项目已归档"));
    }

    #[tokio::test]
    async fn create_task_rejects_project_owned_by_another_user() {
        let service = test_service().await;
        let creator = agent_user("owner-b");
        save_project(
            &service,
            "project-owned-by-a",
            "owner-a",
            TaskProjectStatus::Active,
        )
        .await;

        let err = create_task_with_project(&service, Some("project-owned-by-a"), Some(&creator))
            .await
            .expect_err("other owner's project should be rejected");

        assert!(err.contains("无权访问"));
    }

    #[tokio::test]
    async fn create_task_allows_prerequisites_from_same_project() {
        let service = test_service().await;
        let creator = agent_user("owner-a");
        save_project(&service, "project-a", "owner-a", TaskProjectStatus::Active).await;
        let prerequisite = create_task_with_project(&service, Some("project-a"), Some(&creator))
            .await
            .expect("create prerequisite");
        let mut request = create_task_request("dependent task");
        request.prerequisite_task_ids = Some(vec![prerequisite.id.clone()]);

        let task = service
            .create_task(
                request,
                Some(&creator),
                Some(TaskSourceContext {
                    project_id: Some("project-a".to_string()),
                    ..TaskSourceContext::default()
                }),
            )
            .await
            .expect("create dependent task");

        assert_eq!(task.project_id, "project-a");
        assert_eq!(task.prerequisite_task_ids, vec![prerequisite.id]);
    }

    #[tokio::test]
    async fn create_task_rejects_prerequisites_from_another_project() {
        let service = test_service().await;
        let creator = agent_user("owner-a");
        save_project(&service, "project-a", "owner-a", TaskProjectStatus::Active).await;
        save_project(&service, "project-b", "owner-a", TaskProjectStatus::Active).await;
        let prerequisite = create_task_with_project(&service, Some("project-a"), Some(&creator))
            .await
            .expect("create prerequisite");
        let mut request = create_task_request("dependent task");
        request.prerequisite_task_ids = Some(vec![prerequisite.id]);

        let err = service
            .create_task(
                request,
                Some(&creator),
                Some(TaskSourceContext {
                    project_id: Some("project-b".to_string()),
                    ..TaskSourceContext::default()
                }),
            )
            .await
            .expect_err("cross-project prerequisite should fail");

        assert!(err.contains("前置任务必须属于同一项目"));
    }

    #[tokio::test]
    async fn set_task_prerequisites_rejects_prerequisites_from_another_project() {
        let service = test_service().await;
        let creator = agent_user("owner-a");
        save_project(&service, "project-a", "owner-a", TaskProjectStatus::Active).await;
        save_project(&service, "project-b", "owner-a", TaskProjectStatus::Active).await;
        let prerequisite = create_task_with_project(&service, Some("project-a"), Some(&creator))
            .await
            .expect("create prerequisite");
        let dependent = create_task_with_project(&service, Some("project-b"), Some(&creator))
            .await
            .expect("create dependent");

        let err = service
            .set_task_prerequisites(dependent.id.as_str(), vec![prerequisite.id], Some(&creator))
            .await
            .expect_err("cross-project prerequisite should fail");

        assert!(err.contains("前置任务必须属于同一项目"));
    }
}
