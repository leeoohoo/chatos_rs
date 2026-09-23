#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask_user_prompt_service::AskUserPromptService;
    use crate::config::{AppConfig, StoreMode};
    use chatos_plugin_management_sdk::TaskPluginConfig;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use crate::models::{TaskMcpConfig, TaskToolState};
    use crate::store::AppStore;

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://verification-repair-test".to_string(),
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
            worker_concurrency: 1,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1000,
            default_tool_results_model_total_max_chars: 2000,
            chatos_callback_url: String::new(),
            chatos_callback_http_client: reqwest::Client::new(),
            chatos_internal_api_secret: None,
            mcp_management_internal_api_secret: None,
            user_service_internal_api_secret: None,
            callback_timeout: Duration::from_millis(1000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(1000),
        }
    }

    async fn test_run_service() -> (RunService, AppStore) {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("create test store");
        let service = RunService::new(
            config,
            store.clone(),
            AskUserPromptService::new(store.clone()),
        );
        (service, store)
    }

    fn task(id: &str, role: &str, owned_paths: &[&str]) -> TaskRecord {
        TaskRecord {
            id: id.to_string(),
            title: id.to_string(),
            description: None,
            objective: id.to_string(),
            input_payload: Some(json!({
                "task_role": role,
                "project_task_id": "project-task-1",
                "execution_group_id": "group-1",
                "owned_paths": owned_paths,
                "acceptance_criteria": ["browser smoke passes"],
            })),
            status: TaskStatus::Succeeded,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: Some("model-1".to_string()),
            memory_thread_id: format!("task-{id}"),
            tenant_id: "tenant-1".to_string(),
            subject_id: "subject-1".to_string(),
            project_id: Some("project-1".to_string()),
            project_context: None,
            task_profile: "execution".to_string(),
            creator_user_id: Some("user-1".to_string()),
            creator_username: Some("user".to_string()),
            creator_display_name: Some("User".to_string()),
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("user".to_string()),
            owner_display_name: Some("User".to_string()),
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: Some("session-1".to_string()),
            source_turn_id: Some("turn-1".to_string()),
            source_user_message_id: Some("group-1".to_string()),
            remote_connection_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: TaskToolState::default(),
            plugin_config: TaskPluginConfig::default(),
            plugin_selection_audit: None,
            mcp_config: TaskMcpConfig::default(),
            created_at: "2026-08-15T00:00:00Z".to_string(),
            updated_at: "2026-08-15T00:00:00Z".to_string(),
            deleted_at: None,
        }
    }

    fn blocked_run(task_id: &str) -> TaskRunRecord {
        let mut run = TaskRunRecord::queued(
            "verification-run-1".to_string(),
            task_id.to_string(),
            "model-1".to_string(),
            format!("task-{task_id}"),
            json!({}),
            "2026-08-15T00:00:00Z".to_string(),
        );
        run.status = TaskRunStatus::Blocked;
        run.error_message = Some("default page is blank".to_string());
        run.report = Some(json!({"verification_evidence": ["#root is empty"]}));
        run
    }

    #[test]
    fn repair_and_reverify_preserve_contract_and_enforce_tool_boundaries() {
        let verification = task("verification-1", "verification", &[]);
        let mut implementation = task("implementation-1", "implementation", &["src"]);
        implementation.mcp_config.enabled_builtin_kinds = vec![
            "CodeMaintainerRead".to_string(),
            "CodeMaintainerWrite".to_string(),
            "TerminalController".to_string(),
        ];
        let run = blocked_run(verification.id.as_str());
        let plan = VerificationRepairPlan {
            project_task_id: "project-task-1".to_string(),
            execution_group_id: "group-1".to_string(),
            owned_paths: vec!["src".to_string()],
            acceptance_criteria: vec!["browser smoke passes".to_string()],
            successful_implementation_prerequisites: vec![implementation],
            repair_attempt: 1,
        };

        let repair = build_repair_task(&verification, &run, &plan, "2026-08-15T01:00:00Z");
        let reverify =
            build_reverify_task(&verification, &run, &plan, &repair, "2026-08-15T01:00:00Z");

        assert_eq!(
            payload_string(&repair, "task_role").as_deref(),
            Some("implementation")
        );
        assert_eq!(payload_strings(&repair, "owned_paths"), vec!["src"]);
        assert!(repair.mcp_config.enabled_builtin_kinds.iter().any(|kind| {
            chatos_mcp_runtime::builtin_kind_by_any(kind)
                == Some(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite)
        }));
        assert_eq!(reverify.prerequisite_task_ids, vec![repair.id.clone()]);
        assert_eq!(
            payload_string(&reverify, "task_role").as_deref(),
            Some("verification")
        );
        assert!(payload_strings(&reverify, "owned_paths").is_empty());
        assert!(!reverify
            .mcp_config
            .enabled_builtin_kinds
            .iter()
            .any(|kind| {
                chatos_mcp_runtime::builtin_kind_by_any(kind)
                    == Some(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite)
            }));
        assert_eq!(
            reverify
                .task_tool_state
                .repair_origin_verification_run_id
                .as_deref(),
            Some("verification-run-1")
        );
    }

    #[test]
    fn second_reverify_attempt_increments_and_stays_bounded() {
        let mut verification = task("verification-2", "verification", &[]);
        verification.task_tool_state.repair_attempt = 1;
        assert_eq!(
            verification.task_tool_state.repair_attempt + 1,
            VERIFICATION_REPAIR_MAX_ATTEMPTS
        );
        verification.task_tool_state.repair_attempt = VERIFICATION_REPAIR_MAX_ATTEMPTS;
        assert!(verification.task_tool_state.repair_attempt >= VERIFICATION_REPAIR_MAX_ATTEMPTS);
    }

    #[tokio::test]
    async fn persisted_repair_chain_is_idempotent_after_restart_style_reentry() {
        let (service, store) = test_run_service().await;
        let verification = task("verification-idempotent", "verification", &[]);
        let mut implementation = task("implementation-idempotent", "implementation", &["src"]);
        implementation.mcp_config.enabled_builtin_kinds = vec![
            "CodeMaintainerRead".to_string(),
            "CodeMaintainerWrite".to_string(),
        ];
        store
            .save_task(implementation.clone())
            .await
            .expect("save implementation task");
        store
            .save_task(verification.clone())
            .await
            .expect("save verification task");
        store
            .set_task_prerequisites(verification.id.as_str(), vec![implementation.id.clone()])
            .await
            .expect("save verification prerequisites");
        let run = blocked_run(verification.id.as_str());
        let plan = service
            .build_verification_repair_plan(&verification, &run)
            .await
            .expect("build repair plan")
            .expect("repair plan");

        let first = service
            .persist_verification_repair_chain(&verification, &run, &plan)
            .await
            .expect("persist initial repair chain");
        assert!(first.created_event_required());
        let second = service
            .persist_verification_repair_chain(&verification, &run, &plan)
            .await
            .expect("recover persisted repair chain");

        assert!(!second.created_event_required());
        assert_eq!(second.repair.id, first.repair.id);
        assert_eq!(second.reverify.id, first.reverify.id);
        let tasks = store
            .list_tasks_filtered(&TaskListFilters {
                project_scope: Some(crate::models::TaskProjectScopeFilter::Project),
                project_id: verification.project_id.clone(),
                include_subtasks: Some(false),
                ..TaskListFilters::default()
            })
            .await
            .expect("list repair tasks");
        let chain_tasks = tasks
            .iter()
            .filter(|task| {
                payload_string(task, REPAIR_ORIGIN_RUN_ID_KEY).as_deref() == Some(run.id.as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(chain_tasks.len(), 2);
        let superseded = store
            .get_task(verification.id.as_str())
            .await
            .expect("load verification task")
            .expect("verification task");
        assert_eq!(superseded.status, TaskStatus::Cancelled);
        assert_eq!(
            superseded.task_tool_state.superseded_by_task_id.as_deref(),
            Some(first.repair.id.as_str())
        );
        assert_eq!(
            superseded.task_tool_state.replacement_task_ids,
            vec![first.repair.id.clone(), first.reverify.id.clone()]
        );
        assert_eq!(
            store
                .list_task_prerequisites(first.reverify.id.as_str())
                .await
                .expect("load reverify prerequisites")
                .into_iter()
                .map(|edge| edge.prerequisite_task_id)
                .collect::<Vec<_>>(),
            vec![first.repair.id]
        );
        assert_eq!(
            verification_repair_chain_event_id(run.id.as_str()),
            "verification_repair_chain_created:verification-run-1"
        );
    }
}
