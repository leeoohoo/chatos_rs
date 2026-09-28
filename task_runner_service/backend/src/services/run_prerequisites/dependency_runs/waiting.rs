// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(super) async fn active_run_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.store
            .latest_run_for_task_by_statuses(
                task_id,
                &[TaskRunStatus::Queued, TaskRunStatus::Running],
            )
            .await
    }

    pub(super) async fn latest_successful_run(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.store
            .latest_run_for_task_by_statuses(task_id, &[TaskRunStatus::Succeeded])
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask_user_prompt_service::AskUserPromptService;
    use crate::config::{AppConfig, StoreMode, TaskRunnerRole};
    use crate::store::AppStore;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            role: TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://dependency-run-lookup-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            memory_engine_http_client: reqwest::Client::new(),
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_secs(1),
            execution_timeout: Duration::from_secs(1),
            scheduler_poll_interval: Duration::from_secs(1),
            worker_id: "test-worker".to_string(),
            worker_claim_ttl: Duration::from_secs(120),
            worker_concurrency: 1,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 2_000,
            chatos_callback_url: String::new(),
            chatos_callback_http_client: reqwest::Client::new(),
            chatos_internal_api_secret: None,
            mcp_management_internal_api_secret: None,
            user_service_internal_api_secret: None,
            callback_timeout: Duration::from_secs(1),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_secs(1),
        }
    }

    fn run(id: &str, status: TaskRunStatus, created_at: &str) -> TaskRunRecord {
        let mut run = TaskRunRecord::queued(
            id.to_string(),
            "dependency-task".to_string(),
            "model".to_string(),
            "thread".to_string(),
            json!({}),
            created_at.to_string(),
        );
        run.status = status;
        run
    }

    #[tokio::test]
    async fn dependency_waiting_uses_only_targeted_latest_run_queries() {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        store
            .save_run(run(
                "active-run",
                TaskRunStatus::Queued,
                "2026-09-26T00:01:00Z",
            ))
            .await
            .expect("active run");
        store
            .save_run(run(
                "successful-run",
                TaskRunStatus::Succeeded,
                "2026-09-26T00:00:00Z",
            ))
            .await
            .expect("successful run");
        let service = RunService::new(
            config,
            store.clone(),
            AskUserPromptService::new(store.clone()),
        );

        assert_eq!(
            service
                .active_run_for_task("dependency-task")
                .await
                .expect("active lookup")
                .expect("active run")
                .id,
            "active-run"
        );
        assert_eq!(
            service
                .latest_successful_run("dependency-task")
                .await
                .expect("successful lookup")
                .expect("successful run")
                .id,
            "successful-run"
        );

        let AppStore::InMemory(memory) = &store else {
            panic!("expected memory store");
        };
        assert_eq!(memory.run_lookup_query_counts(), (0, 2));
    }

    #[tokio::test]
    async fn dependency_run_lookup_handles_empty_ties_and_mixed_statuses() {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        let service = RunService::new(
            config,
            store.clone(),
            AskUserPromptService::new(store.clone()),
        );

        assert!(service
            .active_run_for_task("empty-task")
            .await
            .expect("empty active lookup")
            .is_none());
        assert!(service
            .latest_successful_run("empty-task")
            .await
            .expect("empty successful lookup")
            .is_none());

        for candidate in [
            run("active-a", TaskRunStatus::Queued, "2026-09-26T00:01:00Z"),
            run("active-z", TaskRunStatus::Running, "2026-09-26T00:01:00Z"),
            run(
                "success-a",
                TaskRunStatus::Succeeded,
                "2026-09-26T00:02:00Z",
            ),
            run(
                "success-z",
                TaskRunStatus::Succeeded,
                "2026-09-26T00:02:00Z",
            ),
            run(
                "failed-newest",
                TaskRunStatus::Failed,
                "2026-09-26T00:03:00Z",
            ),
        ] {
            store.save_run(candidate).await.expect("save run");
        }

        assert_eq!(
            service
                .active_run_for_task("dependency-task")
                .await
                .expect("active lookup")
                .expect("active run")
                .id,
            "active-z"
        );
        assert_eq!(
            service
                .latest_successful_run("dependency-task")
                .await
                .expect("successful lookup")
                .expect("successful run")
                .id,
            "success-z"
        );
    }
}
