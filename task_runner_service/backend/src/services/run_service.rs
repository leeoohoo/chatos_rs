// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::platform_queue::TaskQueueTopology;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TaskRunnerPromptCachePolicy {
    pub(crate) enabled: bool,
    pub(crate) retention_enabled: bool,
}

impl RunService {
    #[cfg(test)]
    pub(crate) fn new(
        config: AppConfig,
        store: AppStore,
        ask_user_prompt_service: AskUserPromptService,
    ) -> Self {
        Self {
            config,
            task_queue_topology: TaskQueueTopology::inline_defaults(),
            store,
            plugin_management_client: None,
            ask_user_prompt_service,
            runtime_stats: crate::state::TaskRunnerRuntimeStats::default(),
            cloud_agent_store: CloudAgentStateStore::memory(),
            start_locks: Arc::new(KeyedAsyncLockRegistry::default()),
            callback_delivery_locks: Arc::new(KeyedAsyncLockRegistry::default()),
            runtime_abort_tokens: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn new_with_plugin_management(
        config: AppConfig,
        task_queue_topology: TaskQueueTopology,
        store: AppStore,
        ask_user_prompt_service: AskUserPromptService,
        plugin_management_client: PluginManagementClient,
        runtime_stats: crate::state::TaskRunnerRuntimeStats,
        cloud_agent_store: CloudAgentStateStore,
    ) -> Self {
        Self {
            config,
            task_queue_topology,
            store,
            plugin_management_client: Some(plugin_management_client),
            ask_user_prompt_service,
            runtime_stats,
            cloud_agent_store,
            start_locks: Arc::new(KeyedAsyncLockRegistry::default()),
            callback_delivery_locks: Arc::new(KeyedAsyncLockRegistry::default()),
            runtime_abort_tokens: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    pub(super) async fn effective_task_execution_max_iterations(&self) -> Result<usize, String> {
        Ok(self
            .effective_task_runner_runtime_settings()
            .await?
            .max_iterations)
    }

    pub(super) async fn effective_task_runner_runtime_settings(
        &self,
    ) -> Result<chatos_agent::TaskRunnerRuntimeSettings, String> {
        let snapshot = load_managed_config_snapshot().await?;
        chatos_agent::require_task_runner_runtime_settings(&snapshot)
    }

    pub(super) async fn effective_run_timeouts_ms(&self) -> Result<(u64, u64), String> {
        let snapshot = load_managed_config_snapshot().await?;
        Ok((
            require_managed_u64(&snapshot, TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY, 1)?,
            require_managed_u64(&snapshot, TASK_RUNNER_AI_READ_TIMEOUT_CONFIG_KEY, 1)?,
        ))
    }

    pub(super) async fn effective_ai_read_timeout_ms(&self) -> Result<u64, String> {
        let snapshot = load_managed_config_snapshot().await?;
        require_managed_u64(&snapshot, TASK_RUNNER_AI_READ_TIMEOUT_CONFIG_KEY, 1)
    }

    pub(super) async fn effective_node_supply_chain_policy(
        &self,
    ) -> Result<super::run_model_phase::supply_chain::NodeSupplyChainPolicy, String> {
        let snapshot = load_managed_config_snapshot().await?;
        let audit_level = require_managed_string(
            &snapshot,
            TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_LEVEL_CONFIG_KEY,
        )?
        .to_ascii_lowercase();
        if audit_level != "high" {
            return Err(format!(
                "managed configuration key {} must be high",
                TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_LEVEL_CONFIG_KEY
            ));
        }
        Ok(
            super::run_model_phase::supply_chain::NodeSupplyChainPolicy {
                baseline_revision: require_managed_string(
                    &snapshot,
                    TASK_RUNNER_SUPPLY_CHAIN_BASELINE_REVISION_CONFIG_KEY,
                )?,
                dependency_requirements: require_managed_string_map(
                    &snapshot,
                    TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY,
                )?,
                audit_level,
                install_script_allowlist: require_managed_string_set(
                    &snapshot,
                    TASK_RUNNER_SUPPLY_CHAIN_INSTALL_SCRIPT_ALLOWLIST_CONFIG_KEY,
                )?,
                install_registry: require_managed_string(
                    &snapshot,
                    TASK_RUNNER_SUPPLY_CHAIN_NODE_INSTALL_REGISTRY_CONFIG_KEY,
                )?,
                audit_registry: require_managed_string(
                    &snapshot,
                    TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_REGISTRY_CONFIG_KEY,
                )?,
            },
        )
    }

    pub(super) async fn effective_prompt_cache_policy(
        &self,
    ) -> Result<TaskRunnerPromptCachePolicy, String> {
        let snapshot = load_managed_config_snapshot().await?;
        prompt_cache_policy_from_snapshot(&snapshot)
    }

    pub(super) async fn effective_tool_result_model_budget_limits(
        &self,
    ) -> Result<ToolResultModelBudgetLimits, String> {
        let snapshot = load_managed_config_snapshot().await?;
        let per_result =
            require_managed_usize(&snapshot, TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY, 1)?;
        let total = require_managed_usize(
            &snapshot,
            TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY,
            1,
        )?;
        if total < per_result {
            return Err(format!(
                "managed configuration key {} must be greater than or equal to {}",
                TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY,
                TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY
            ));
        }
        Ok(ToolResultModelBudgetLimits::new(per_result, total))
    }

    pub async fn list_runs(&self, task_id: Option<&str>) -> Result<Vec<TaskRunRecord>, String> {
        self.store.list_runs(task_id).await
    }

    pub async fn execution_stats(&self) -> Result<RunExecutionStats, String> {
        self.store.run_execution_stats().await
    }

    pub fn runtime_stats(&self) -> &crate::state::TaskRunnerRuntimeStats {
        &self.runtime_stats
    }

    #[cfg(test)]
    pub(crate) fn store(&self) -> &AppStore {
        &self.store
    }

    pub async fn list_runs_filtered(
        &self,
        filters: RunListFilters,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let filters = sanitize_run_list_filters(filters);
        self.store.list_runs_filtered(&filters).await
    }

    pub async fn list_runs_page(
        &self,
        filters: RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        let mut filters = sanitize_run_list_filters(filters);
        filters.limit = Some(filters.limit.unwrap_or(20));
        filters.offset = Some(filters.offset.unwrap_or(0));
        self.store.list_runs_page(&filters).await
    }

    pub async fn run_index(
        &self,
        filters: RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let filters = sanitize_run_list_filters(filters);
        self.store.list_run_summaries_filtered(&filters).await
    }

    pub async fn get_run_summaries_by_ids(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let ids = sanitize_id_list(ids);
        self.store.get_run_summaries_by_ids(&ids).await
    }

    pub async fn get_run(&self, id: &str) -> Result<Option<TaskRunRecord>, String> {
        self.store.get_run(id).await
    }

    pub async fn has_active_run_for_task(&self, task_id: &str) -> Result<bool, String> {
        self.store.has_active_run_for_task(task_id).await
    }

    pub(crate) fn signal_runtime_cancel(&self, run_id: &str) {
        self.store.signal_local_run_abort(run_id);
        if let Some(token) = self.runtime_abort_tokens.lock().get(run_id).cloned() {
            token.cancel();
        }
    }

    pub(super) fn register_runtime_abort_token(
        &self,
        run_id: &str,
        token: tokio_util::sync::CancellationToken,
    ) {
        self.runtime_abort_tokens
            .lock()
            .insert(run_id.to_string(), token.clone());
        if self.store.is_cancel_requested(run_id) {
            token.cancel();
        }
    }

    pub(super) fn unregister_runtime_abort_token(&self, run_id: &str) {
        self.runtime_abort_tokens.lock().remove(run_id);
    }

    pub(crate) fn signal_ask_user_resolved(&self, prompt_id: &str) {
        self.ask_user_prompt_service
            .signal_prompt_resolved(prompt_id);
    }

    pub async fn batch_start_runs(
        &self,
        request: BatchTaskRunRequest,
    ) -> Result<BatchTaskOperationResponse, String> {
        self.batch_start_runs_with_user(request, None).await
    }

    pub async fn batch_start_runs_for_user(
        &self,
        request: BatchTaskRunRequest,
        current_user: &CurrentUser,
    ) -> Result<BatchTaskOperationResponse, String> {
        self.batch_start_runs_with_user(request, Some(current_user))
            .await
    }

    async fn batch_start_runs_with_user(
        &self,
        request: BatchTaskRunRequest,
        current_user: Option<&CurrentUser>,
    ) -> Result<BatchTaskOperationResponse, String> {
        let task_ids = normalize_batch_task_ids(request.task_ids)?;
        let mut results = Vec::with_capacity(task_ids.len());

        for task_id in task_ids {
            let run_result = if let Some(current_user) = current_user {
                self.start_run_for_user(
                    &task_id,
                    StartTaskRunRequest {
                        model_config_id: request.model_config_id.clone(),
                        prompt_override: request.prompt_override.clone(),
                        retry_instruction: None,
                    },
                    current_user,
                )
                .await
            } else {
                self.start_run(
                    &task_id,
                    StartTaskRunRequest {
                        model_config_id: request.model_config_id.clone(),
                        prompt_override: request.prompt_override.clone(),
                        retry_instruction: None,
                    },
                )
                .await
            };
            match run_result {
                Ok(run) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: true,
                    message: None,
                    run_id: Some(run.id),
                }),
                Err(err) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some(err),
                    run_id: None,
                }),
            }
        }

        Ok(summarize_batch_results(results))
    }

    pub fn subscribe_run_events(&self) -> broadcast::Receiver<TaskRunEventRecord> {
        self.store.subscribe_run_events()
    }

    pub fn broadcast_run_event(&self, event: TaskRunEventRecord) {
        self.store.broadcast_run_event(event);
    }

    pub async fn list_run_events(&self, run_id: &str) -> Result<Vec<TaskRunEventRecord>, String> {
        self.store.list_run_events(run_id).await
    }

    pub async fn get_run_event(
        &self,
        run_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskRunEventRecord>, String> {
        self.store.get_run_event(run_id, event_id).await
    }

    pub async fn list_run_events_after(
        &self,
        run_id: &str,
        after_created_at: Option<&str>,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TaskRunEventRecord>, String> {
        self.store
            .list_run_events_after(run_id, after_created_at, after_id, limit)
            .await
    }

    pub async fn latest_run_event_cursor(
        &self,
        run_id: &str,
    ) -> Result<Option<(String, String)>, String> {
        self.store.latest_run_event_cursor(run_id).await
    }

    pub async fn prune_terminal_run_events_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> Result<RunEventPruneResult, String> {
        self.store
            .prune_terminal_run_events_before(cutoff, candidate_limit)
            .await
    }

    pub(crate) fn cloud_agent_store(&self) -> chatos_cloud_agent_runtime::CloudAgentStateStore {
        self.cloud_agent_store.clone()
    }

    pub(crate) async fn enqueue_run_cancel_event_if_needed(
        &self,
        run: &TaskRunRecord,
    ) -> Result<bool, String> {
        if !run.cancel_event_pending || run.status != TaskRunStatus::Running {
            return Ok(false);
        }
        crate::worker_control_queue::publish_run_cancel_event(&self.task_queue_topology, run)
            .await?;
        self.store
            .acknowledge_run_cancel_event(run.id.as_str())
            .await?;
        Ok(true)
    }

    pub(crate) async fn publish_pending_run_cancel_events(
        &self,
        limit: usize,
    ) -> Result<usize, String> {
        let pending = self.store.list_pending_run_cancel_events(limit).await?;
        let mut published = 0usize;
        for run in pending {
            if self.enqueue_run_cancel_event_if_needed(&run).await? {
                published += 1;
            }
        }
        Ok(published)
    }

    pub(crate) async fn publish_pending_run_terminal_events(
        &self,
        limit: usize,
    ) -> Result<usize, String> {
        let pending = self
            .store
            .list_pending_run_terminal_subscriptions(limit)
            .await?;
        let mut published = 0usize;
        for (run, subscription) in pending {
            if subscription.worker_id == "cloud-agent" {
                crate::cloud_agent_queue::publish_dependency_resume(
                    &self.task_queue_topology,
                    self,
                    subscription.parent_run_id.as_str(),
                    run.id.as_str(),
                )
                .await?;
            } else {
                crate::worker_control_queue::publish_run_terminal_event(
                    &self.task_queue_topology,
                    &run,
                    &subscription,
                )
                .await?;
            }
            self.store
                .acknowledge_run_terminal_subscription(subscription.id.as_str())
                .await?;
            published += 1;
        }
        Ok(published)
    }
}

fn prompt_cache_policy_from_snapshot(
    snapshot: &chatos_config_sdk::ConfigSnapshot,
) -> Result<TaskRunnerPromptCachePolicy, String> {
    let enabled = snapshot
        .bool(chatos_agent::TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "missing or invalid managed configuration key {}",
                chatos_agent::TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY
            )
        })?;
    let retention_enabled = snapshot
        .bool(chatos_agent::TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY)
        .ok_or_else(|| {
            format!(
                "missing or invalid managed configuration key {}",
                chatos_agent::TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY
            )
        })?;
    Ok(TaskRunnerPromptCachePolicy {
        enabled,
        retention_enabled,
    })
}

#[cfg(test)]
mod worker_claim_tests {
    use super::*;
    use std::collections::BTreeMap;

    use serde_json::json;

    fn prompt_cache_snapshot(
        values: BTreeMap<String, serde_json::Value>,
    ) -> chatos_config_sdk::ConfigSnapshot {
        chatos_config_sdk::ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "task-runner".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values,
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: Some("configuration_center".to_string()),
        }
    }

    #[test]
    fn prompt_cache_policy_requires_both_managed_values() {
        let snapshot = prompt_cache_snapshot(BTreeMap::from([(
            chatos_agent::TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY.to_string(),
            json!(true),
        )]));

        let error = prompt_cache_policy_from_snapshot(&snapshot).expect_err("missing retention");

        assert!(error.contains(chatos_agent::TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY));
    }

    #[test]
    fn prompt_cache_policy_uses_only_managed_values() {
        let snapshot = prompt_cache_snapshot(BTreeMap::from([
            (
                chatos_agent::TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY.to_string(),
                json!(true),
            ),
            (
                chatos_agent::TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY.to_string(),
                json!(false),
            ),
        ]));

        assert_eq!(
            prompt_cache_policy_from_snapshot(&snapshot).expect("managed policy"),
            TaskRunnerPromptCachePolicy {
                enabled: true,
                retention_enabled: false,
            }
        );
    }
}
