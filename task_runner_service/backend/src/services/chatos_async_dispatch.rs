// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::future::Future;
use std::pin::Pin;

use crate::models::TaskScheduleConfig;
use tracing::warn;

use super::*;

impl RunService {
    pub(crate) async fn set_project_execution_paused(
        &self,
        tasks: &[TaskRecord],
        paused: bool,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let mut task_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
        task_ids.sort();
        task_ids.dedup();
        let mut start_guards = Vec::with_capacity(task_ids.len());
        for task_id in &task_ids {
            start_guards.push(
                self.start_lock_for_task(task_id.as_str())
                    .lock_owned()
                    .await,
            );
        }
        self.store
            .set_tasks_execution_paused(task_ids.as_slice(), paused)
            .await?;
        self.store
            .set_queued_runs_dispatch_paused(task_ids.as_slice(), paused)
            .await?;
        drop(start_guards);
        if paused {
            return Ok(Vec::new());
        }
        if let Err(err) = self
            .enqueue_queued_runs_for_tasks(task_ids.as_slice())
            .await
        {
            warn!(
                task_count = task_ids.len(),
                error = err.as_str(),
                "failed to enqueue resumed queued runs for rabbitmq dispatch"
            );
        }
        let mut refreshed_tasks = Vec::with_capacity(task_ids.len());
        for task_id in &task_ids {
            if let Some(task) = self.store.get_task(task_id.as_str()).await? {
                refreshed_tasks.push(task);
            }
        }
        self.dispatch_ready_chatos_async_tasks(refreshed_tasks.as_slice())
            .await
    }

    pub(crate) fn dispatch_confirmed_project_execution_tasks<'a>(
        &'a self,
        tasks: &'a [TaskRecord],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TaskRunRecord>, String>> + Send + 'a>> {
        Box::pin(async move {
            for task in tasks
                .iter()
                .filter(|task| task.mcp_config.requires_execution)
            {
                self.validate_sandbox_route_for_task(task).await?;
            }
            let activated_at = now_rfc3339();
            let mut activated_tasks = Vec::with_capacity(tasks.len());
            for task in tasks {
                let mut task = self
                    .store
                    .get_task(task.id.as_str())
                    .await?
                    .ok_or_else(|| format!("task not found: {}", task.id))?;
                task.schedule = TaskScheduleConfig {
                    mode: TaskScheduleMode::ContactAsync,
                    run_at: Some(activated_at.clone()),
                    interval_seconds: None,
                    // The dedicated DAG dispatcher starts roots and unlocks
                    // dependants only after every prerequisite has succeeded.
                    // Keeping next_run_at empty prevents the global scheduler
                    // from bypassing that dependency gate.
                    next_run_at: None,
                    last_scheduled_at: task.schedule.last_scheduled_at.clone(),
                };
                task.updated_at = now_rfc3339();
                activated_tasks.push(self.store.save_task(task).await?);
            }
            self.dispatch_ready_chatos_async_tasks(activated_tasks.as_slice())
                .await
        })
    }

    pub(crate) fn dispatch_ready_chatos_async_tasks<'a>(
        &'a self,
        tasks: &'a [TaskRecord],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TaskRunRecord>, String>> + Send + 'a>> {
        Box::pin(async move {
            let mut runs = Vec::new();
            for task in tasks {
                let task = self.hydrate_task_prerequisites(task.clone()).await?;
                if !self.should_dispatch_chatos_async_task(&task) {
                    continue;
                }
                if !self
                    .task_prerequisites_have_succeeded(&task.prerequisite_task_ids)
                    .await?
                {
                    self.consume_chatos_async_schedule_slot(task.id.as_str())
                        .await?;
                    continue;
                }
                if let Some(run) = self
                    .dispatch_ready_chatos_async_task(task.id.as_str())
                    .await?
                {
                    runs.push(run);
                }
            }
            Ok(runs)
        })
    }

    pub(crate) fn dispatch_ready_chatos_async_tasks_for_source_task<'a>(
        &'a self,
        task: &'a TaskRecord,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TaskRunRecord>, String>> + Send + 'a>> {
        Box::pin(async move {
            if task.schedule.mode != TaskScheduleMode::ContactAsync {
                return Ok(Vec::new());
            }
            let Some(source_session_id) = normalized_optional(task.source_session_id.clone())
            else {
                return Ok(Vec::new());
            };
            let source_user_message_id = normalized_optional(task.source_user_message_id.clone());
            let source_turn_id = normalized_optional(task.source_turn_id.clone());
            if source_user_message_id.is_none() && source_turn_id.is_none() {
                return Ok(Vec::new());
            }

            let tasks = self
                .store
                .list_tasks_filtered(&TaskListFilters {
                    project_id: Some(task.project_id.clone()),
                    source_session_id: Some(source_session_id),
                    source_user_message_ids: source_user_message_id.into_iter().collect(),
                    source_turn_ids: source_turn_id.into_iter().collect(),
                    task_profile: Some(task.task_profile.clone()),
                    include_subtasks: Some(false),
                    ..TaskListFilters::default()
                })
                .await?;
            self.dispatch_ready_chatos_async_tasks(tasks.as_slice())
                .await
        })
    }

    async fn dispatch_ready_chatos_async_task(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        if self.has_active_run_for_task(task_id).await? {
            self.consume_chatos_async_schedule_slot(task_id).await?;
            return Ok(None);
        }

        let now = Utc::now();
        match self
            .start_scheduled_run(task_id, StartTaskRunRequest::default())
            .await
        {
            Ok(run) => {
                self.consume_chatos_async_schedule_slot_at(task_id, now)
                    .await?;
                Ok(Some(run))
            }
            Err(err) if is_chatos_async_active_run_conflict_error(err.as_str()) => {
                self.consume_chatos_async_schedule_slot(task_id).await?;
                Ok(None)
            }
            Err(err) => {
                self.mark_chatos_async_schedule_failed(task_id, &err)
                    .await?;
                Err(err)
            }
        }
    }

    async fn hydrate_task_prerequisites(&self, mut task: TaskRecord) -> Result<TaskRecord, String> {
        task.prerequisite_task_ids = self
            .store
            .list_task_prerequisites(task.id.as_str())
            .await?
            .into_iter()
            .map(|item| item.prerequisite_task_id)
            .collect();
        Ok(task)
    }

    fn should_dispatch_chatos_async_task(&self, task: &TaskRecord) -> bool {
        task.schedule.mode == TaskScheduleMode::ContactAsync
            && task.status == TaskStatus::Ready
            && !task.task_tool_state.execution_paused
    }

    async fn task_prerequisites_have_succeeded(
        &self,
        prerequisite_task_ids: &[String],
    ) -> Result<bool, String> {
        for prerequisite_task_id in prerequisite_task_ids {
            let Some(task) = self.store.get_task(prerequisite_task_id).await? else {
                return Ok(false);
            };
            if task.status != TaskStatus::Succeeded {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn consume_chatos_async_schedule_slot(&self, task_id: &str) -> Result<(), String> {
        self.consume_chatos_async_schedule_slot_at(task_id, Utc::now())
            .await
    }

    async fn consume_chatos_async_schedule_slot_at(
        &self,
        task_id: &str,
        started_at: DateTime<Utc>,
    ) -> Result<(), String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Ok(());
        };
        task.schedule = advance_task_schedule_after_dispatch(&task.schedule, started_at)?;
        task.updated_at = now_rfc3339();
        self.store.save_task(task).await?;
        Ok(())
    }

    async fn mark_chatos_async_schedule_failed(
        &self,
        task_id: &str,
        error: &str,
    ) -> Result<(), String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Ok(());
        };
        task.result_summary = normalized_optional(Some(format!("scheduler error: {error}")));
        task.updated_at = now_rfc3339();
        self.store.save_task(task).await?;
        Ok(())
    }
}

fn is_chatos_async_active_run_conflict_error(error: &str) -> bool {
    error.contains("active run already exists") || error.contains("已有正在执行")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask_user_prompt_service::AskUserPromptService;
    use crate::config::{AppConfig, StoreMode, TaskRunnerRole};
    use crate::models::{ModelConfigRecord, TaskMcpConfig, TaskToolState, PUBLIC_PROJECT_ID};
    use crate::store::AppStore;
    use chatos_plugin_management_sdk::TaskPluginConfig;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            role: TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://chatos-parallel-dispatch-test".to_string(),
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
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 2,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 2_000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_http_client: reqwest::Client::new(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: String::new(),
            chatos_callback_http_client: reqwest::Client::new(),
            internal_api_secret: None,
            chatos_internal_api_secret: None,
            mcp_management_internal_api_secret: None,
            user_service_internal_api_secret: None,
            local_connector_internal_api_secret: None,
            local_connector_service_base_url: Some("http://127.0.0.1:39230".to_string()),
            local_connector_http_client: reqwest::Client::new(),
            local_connector_service_request_timeout: Duration::from_millis(5_000),
            plugin_relay_request_timeout: Duration::from_millis(60_000),
            plugin_hook_relay_timeout: Duration::from_millis(330_000),
            plugin_connector_discovery_timeout: Duration::from_millis(10_000),
            callback_timeout: Duration::from_secs(1),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_secs(1),
            project_service_base_url: None,
            project_service_internal_base_url: None,
            project_service_internal_http_client: reqwest::Client::new(),
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_secs(1),
        }
    }

    fn model_config() -> ModelConfigRecord {
        let now = now_rfc3339();
        ModelConfigRecord {
            id: "model-1".to_string(),
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            name: "test model".to_string(),
            provider: "openai".to_string(),
            prompt_vendor: None,
            base_url: "https://example.invalid/v1".to_string(),
            api_key: "test".to_string(),
            model: "test-model".to_string(),
            usage_scenario: None,
            temperature: None,
            max_output_tokens: None,
            model_request_max_retries: 0,
            thinking_level: None,
            supports_images: false,
            supports_reasoning: false,
            supports_responses: true,
            instructions: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn ready_task(id: &str) -> TaskRecord {
        let now = now_rfc3339();
        TaskRecord {
            id: id.to_string(),
            title: format!("task {id}"),
            description: None,
            objective: format!("complete {id}"),
            input_payload: None,
            status: TaskStatus::Ready,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: Some("model-1".to_string()),
            memory_thread_id: format!("memory-{id}"),
            tenant_id: "tenant".to_string(),
            subject_id: "subject".to_string(),
            project_id: PUBLIC_PROJECT_ID.to_string(),
            task_profile: crate::models::TASK_PROFILE_DEFAULT.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig {
                mode: TaskScheduleMode::ContactAsync,
                ..TaskScheduleConfig::default()
            },
            parent_task_id: None,
            source_run_id: None,
            source_session_id: Some("session-1".to_string()),
            source_turn_id: Some("turn-1".to_string()),
            source_user_message_id: Some("message-1".to_string()),
            prerequisite_task_ids: Vec::new(),
            task_tool_state: TaskToolState::default(),
            plugin_config: TaskPluginConfig::default(),
            mcp_config: TaskMcpConfig {
                requires_execution: false,
                ..TaskMcpConfig::default()
            },
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn dispatches_all_ready_independent_tasks_in_the_same_dag_wave() {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        store
            .save_model_config(model_config())
            .await
            .expect("save model");
        let first = store.save_task(ready_task("first")).await.expect("first");
        let second = store.save_task(ready_task("second")).await.expect("second");
        let service = RunService::new(
            config,
            store.clone(),
            AskUserPromptService::new(store.clone()),
        );

        let runs = service
            .dispatch_ready_chatos_async_tasks(&[first, second])
            .await
            .expect("dispatch DAG wave");

        assert_eq!(runs.len(), 2);
        assert_ne!(runs[0].task_id, runs[1].task_id);
        assert_eq!(store.list_runs(None).await.expect("runs").len(), 2);
    }

    #[tokio::test]
    async fn blocked_prerequisite_does_not_release_dependent_task() {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        store
            .save_model_config(model_config())
            .await
            .expect("save model");
        let mut prerequisite = ready_task("blocked-prerequisite");
        prerequisite.status = TaskStatus::Blocked;
        store
            .save_task(prerequisite)
            .await
            .expect("save prerequisite");
        let dependent = store
            .save_task(ready_task("dependent"))
            .await
            .expect("save dependent");
        store
            .set_task_prerequisites(
                dependent.id.as_str(),
                vec!["blocked-prerequisite".to_string()],
            )
            .await
            .expect("save prerequisites");
        let service = RunService::new(
            config,
            store.clone(),
            AskUserPromptService::new(store.clone()),
        );

        let runs = service
            .dispatch_ready_chatos_async_tasks(&[dependent])
            .await
            .expect("dispatch DAG wave");

        assert!(runs.is_empty());
        assert!(store.list_runs(None).await.expect("runs").is_empty());
    }
}
