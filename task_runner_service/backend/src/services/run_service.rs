// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::platform_queue::{TaskQueueMode, TaskQueueTopology};
use tracing::warn;

const MIN_WORKER_CLAIM_EXPIRY_GRACE: Duration = Duration::from_secs(5);
const MAX_WORKER_CLAIM_EXPIRY_GRACE: Duration = Duration::from_secs(30);
const MAX_WORKER_CLAIM_ATTEMPTS: i64 = 3;
const WORKER_CLAIM_EXPIRED_ERROR: &str = "worker claim expired";
const CANCEL_REQUESTED_CLAIM_EXPIRED_REASON: &str =
    "run cancellation requested before worker claim expired";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RejectedRunClaimHeartbeatAction {
    Continue,
    Stop,
    Abort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TaskRunnerPromptCachePolicy {
    pub(crate) enabled: bool,
    pub(crate) retention_enabled: bool,
}

pub(crate) fn worker_claim_expiry_grace(claim_ttl: Duration) -> Duration {
    let proportional =
        Duration::from_millis((claim_ttl.as_millis() / 10).min(u64::MAX as u128) as u64);
    proportional.clamp(MIN_WORKER_CLAIM_EXPIRY_GRACE, MAX_WORKER_CLAIM_EXPIRY_GRACE)
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
            start_locks: Arc::new(KeyedAsyncLockRegistry::default()),
            callback_delivery_locks: Arc::new(KeyedAsyncLockRegistry::default()),
            runtime_abort_tokens: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            run_terminal_waiters: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            plugin_cloud_bundle_cache: Arc::new(parking_lot::Mutex::new(Default::default())),
        }
    }

    pub(crate) fn new_with_plugin_management(
        config: AppConfig,
        task_queue_topology: TaskQueueTopology,
        store: AppStore,
        ask_user_prompt_service: AskUserPromptService,
        plugin_management_client: PluginManagementClient,
        runtime_stats: crate::state::TaskRunnerRuntimeStats,
    ) -> Self {
        Self {
            config,
            task_queue_topology,
            store,
            plugin_management_client: Some(plugin_management_client),
            ask_user_prompt_service,
            runtime_stats,
            start_locks: Arc::new(KeyedAsyncLockRegistry::default()),
            callback_delivery_locks: Arc::new(KeyedAsyncLockRegistry::default()),
            runtime_abort_tokens: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            run_terminal_waiters: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            plugin_cloud_bundle_cache: Arc::new(parking_lot::Mutex::new(Default::default())),
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
                audit_level,
                install_script_allowlist: require_managed_string_set(
                    &snapshot,
                    TASK_RUNNER_SUPPLY_CHAIN_INSTALL_SCRIPT_ALLOWLIST_CONFIG_KEY,
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

    pub(super) async fn effective_execution_timeout(&self) -> Result<Duration, String> {
        let snapshot = load_managed_config_snapshot().await?;
        Ok(Duration::from_millis(require_managed_u64(
            &snapshot,
            TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY,
            1,
        )?))
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

    pub(super) async fn effective_execution_environment_mode(&self) -> Result<String, String> {
        let snapshot = load_managed_config_snapshot().await?;
        require_managed_execution_environment_mode(&snapshot)
    }

    pub(super) async fn effective_sandbox_enabled(&self) -> Result<bool, String> {
        let snapshot = load_managed_config_snapshot().await?;
        require_managed_bool(&snapshot, TASK_RUNNER_SANDBOX_ENABLED_CONFIG_KEY)
    }

    pub(super) async fn effective_sandbox_manager_base_url(&self) -> Result<String, String> {
        let snapshot = load_managed_config_snapshot().await?;
        require_managed_http_base_url(&snapshot, TASK_RUNNER_SANDBOX_MANAGER_BASE_URL_CONFIG_KEY)
    }

    pub(super) async fn effective_sandbox_lease_ttl_seconds(&self) -> Result<u64, String> {
        let snapshot = load_managed_config_snapshot().await?;
        require_managed_u64(
            &snapshot,
            TASK_RUNNER_SANDBOX_LEASE_TTL_SECONDS_CONFIG_KEY,
            1,
        )
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

    pub async fn claim_next_queued_run(
        &self,
        worker_id: &str,
        claim_ttl: Duration,
    ) -> Result<Option<TaskRunRecord>, String> {
        let claim_token = Uuid::new_v4().to_string();
        let claim_until = (chrono::Utc::now()
            + chrono::Duration::from_std(claim_ttl).map_err(|err| err.to_string())?)
        .to_rfc3339();
        self.store
            .claim_next_queued_run(worker_id, claim_token.as_str(), claim_until.as_str())
            .await
    }

    pub async fn has_queued_run_waiting_for_execution(&self) -> Result<bool, String> {
        self.store.has_queued_run_waiting_for_execution().await
    }

    pub async fn renew_run_claim(
        &self,
        run: &TaskRunRecord,
        worker_id: &str,
        claim_ttl: Duration,
    ) -> Result<bool, String> {
        let Some(claim_token) = run.claim_token.as_deref() else {
            return Ok(false);
        };
        let claim_until = (chrono::Utc::now()
            + chrono::Duration::from_std(claim_ttl).map_err(|err| err.to_string())?)
        .to_rfc3339();
        let renewed = self
            .store
            .renew_run_claim(&run.id, worker_id, claim_token, claim_until.as_str())
            .await?;
        if renewed {
            if let Err(error) = self.renew_active_sandbox_lease(run.id.as_str()).await {
                warn!(
                    run_id = run.id.as_str(),
                    error = error.as_str(),
                    "task runner failed to renew active sandbox lease"
                );
            }
        }
        Ok(renewed)
    }

    pub async fn reconcile_expired_run_claims(&self, claim_ttl: Duration) -> Result<usize, String> {
        let now = now_rfc3339();
        let expiry_cutoff = (chrono::Utc::now()
            - chrono::Duration::from_std(worker_claim_expiry_grace(claim_ttl))
                .map_err(|err| err.to_string())?)
        .to_rfc3339();
        let reconciled_runs = self
            .store
            .reconcile_expired_run_claims(
                expiry_cutoff.as_str(),
                now.as_str(),
                MAX_WORKER_CLAIM_ATTEMPTS,
            )
            .await?;
        for run in &reconciled_runs {
            self.store.signal_local_run_abort(run.id.as_str());
            let cancelled_after_request = run.status == TaskRunStatus::Cancelled;
            let requeued_after_interruption = run.status == TaskRunStatus::Queued;
            if let Err(err) = self
                .ask_user_prompt_service
                .cancel_pending_prompts_for_run(
                    run.id.as_str(),
                    if cancelled_after_request {
                        CANCEL_REQUESTED_CLAIM_EXPIRED_REASON
                    } else if requeued_after_interruption {
                        "run execution was interrupted and automatically requeued"
                    } else {
                        WORKER_CLAIM_EXPIRED_ERROR
                    },
                )
                .await
            {
                tracing::warn!(
                    run_id = run.id.as_str(),
                    error = err.as_str(),
                    "failed to cancel pending ask user prompts after worker claim expired"
                );
            }
            if let Some(mut task) = self.store.get_task(run.task_id.as_str()).await? {
                if task.last_run_id.as_deref() == Some(run.id.as_str()) {
                    task.status = if cancelled_after_request {
                        TaskStatus::Cancelled
                    } else if requeued_after_interruption {
                        TaskStatus::Queued
                    } else {
                        TaskStatus::Failed
                    };
                    task.result_summary = run.result_summary.clone();
                    task.updated_at = now.clone();
                    self.store.save_task(task).await?;
                }
            }
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    run.id.clone(),
                    if cancelled_after_request {
                        "run.cancel_requested.claim_expired"
                    } else if requeued_after_interruption {
                        "run.claim.expired.requeued"
                    } else {
                        "run.claim.expired"
                    }
                    .to_string(),
                    run.result_summary.clone(),
                    Some(serde_json::json!({
                        "reason": if cancelled_after_request {
                            CANCEL_REQUESTED_CLAIM_EXPIRED_REASON
                        } else if requeued_after_interruption {
                            "worker_claim_expired_requeued"
                        } else {
                            "worker_claim_expired"
                        },
                        "previous_worker_id": run.worker_id,
                        "attempt": run.attempt,
                        "max_attempts": MAX_WORKER_CLAIM_ATTEMPTS,
                    })),
                ))
                .await?;
            if let Err(err) = self.release_sandboxes_for_terminal_run(run).await {
                tracing::warn!(
                    run_id = run.id.as_str(),
                    error = err.as_str(),
                    "failed to release sandboxes after worker claim expired"
                );
            }
            if !requeued_after_interruption {
                self.try_send_terminal_callback(run.task_id.as_str(), run)
                    .await;
            } else if let Err(err) = self.enqueue_run_dispatch_if_needed(run).await {
                warn!(
                    run_id = run.id.as_str(),
                    task_id = run.task_id.as_str(),
                    error = err.as_str(),
                    "failed to re-enqueue recovered run dispatch"
                );
            }
        }
        self.store.refresh_runtime_guards().await?;
        Ok(reconciled_runs.len())
    }

    pub(crate) fn signal_local_run_abort(&self, run_id: &str) {
        self.store.signal_local_run_abort(run_id);
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

    pub(super) fn register_run_terminal_waiter(
        &self,
        run_id: &str,
        parent_run_id: &str,
        token: tokio_util::sync::CancellationToken,
    ) {
        self.run_terminal_waiters
            .lock()
            .insert((run_id.to_string(), parent_run_id.to_string()), token);
    }

    pub(super) fn unregister_run_terminal_waiter(&self, run_id: &str, parent_run_id: &str) {
        self.run_terminal_waiters
            .lock()
            .remove(&(run_id.to_string(), parent_run_id.to_string()));
    }

    pub(crate) fn signal_run_terminal(&self, run_id: &str) {
        let tokens = self
            .run_terminal_waiters
            .lock()
            .iter()
            .filter(|((waiting_run_id, _), _)| waiting_run_id == run_id)
            .map(|(_, token)| token.clone())
            .collect::<Vec<_>>();
        for token in tokens {
            token.cancel();
        }
    }

    pub(crate) fn signal_ask_user_resolved(&self, prompt_id: &str) {
        self.ask_user_prompt_service
            .signal_prompt_resolved(prompt_id);
    }

    pub(crate) fn clear_local_run_abort(&self, run_id: &str) {
        self.store.clear_local_run_abort(run_id);
    }

    pub(crate) async fn run_claim_is_current(&self, run: &TaskRunRecord) -> bool {
        let Some(current) = self.store.get_run(run.id.as_str()).await.ok().flatten() else {
            return false;
        };
        current.status == TaskRunStatus::Running
            && current.worker_id.as_deref() == run.worker_id.as_deref()
            && current.claim_token.as_deref() == run.claim_token.as_deref()
    }

    pub(crate) async fn handle_rejected_run_claim_heartbeat(
        &self,
        claimed_run: &TaskRunRecord,
        worker_id: &str,
    ) -> Result<RejectedRunClaimHeartbeatAction, String> {
        let current = self.store.get_run(claimed_run.id.as_str()).await?;
        let action = match current.as_ref() {
            None => RejectedRunClaimHeartbeatAction::Abort,
            Some(current) if current.status == TaskRunStatus::Running => {
                if current.worker_id.as_deref() == Some(worker_id)
                    && current.claim_token.as_deref() == claimed_run.claim_token.as_deref()
                {
                    RejectedRunClaimHeartbeatAction::Continue
                } else {
                    RejectedRunClaimHeartbeatAction::Abort
                }
            }
            Some(current)
                if current.error_message.as_deref() == Some(WORKER_CLAIM_EXPIRED_ERROR) =>
            {
                RejectedRunClaimHeartbeatAction::Abort
            }
            Some(_) => RejectedRunClaimHeartbeatAction::Stop,
        };
        if action != RejectedRunClaimHeartbeatAction::Abort {
            return Ok(action);
        }

        self.store.signal_local_run_abort(claimed_run.id.as_str());
        if let Err(err) = self
            .ask_user_prompt_service
            .cancel_pending_prompts_for_run(
                claimed_run.id.as_str(),
                "run claim lost while task was executing",
            )
            .await
        {
            tracing::warn!(
                run_id = claimed_run.id.as_str(),
                error = err.as_str(),
                "failed to cancel pending ask user prompts after run claim was lost"
            );
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                claimed_run.id.clone(),
                "run.claim.execution_abort_requested".to_string(),
                Some("运行租约已丢失，旧 Worker 已停止继续执行".to_string()),
                Some(serde_json::json!({
                    "reason": "run_claim_lost",
                    "worker_id": worker_id,
                })),
            ))
            .await
        {
            tracing::warn!(
                run_id = claimed_run.id.as_str(),
                error = err.as_str(),
                "failed to append lost-claim execution abort event"
            );
        }
        Ok(RejectedRunClaimHeartbeatAction::Abort)
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

    pub(crate) fn task_queue_topology(&self) -> &TaskQueueTopology {
        &self.task_queue_topology
    }

    pub(crate) async fn enqueue_run_dispatch_if_needed(
        &self,
        run: &TaskRunRecord,
    ) -> Result<bool, String> {
        if run.dispatch_paused
            || self.task_queue_topology.run_dispatch_mode != TaskQueueMode::RabbitMq
        {
            return Ok(false);
        }
        crate::run_dispatch_queue::enqueue_run_dispatch(&self.task_queue_topology, run.id.as_str())
            .await?;
        self.store
            .acknowledge_run_dispatch_event(run.id.as_str())
            .await?;
        Ok(true)
    }

    pub(crate) async fn publish_pending_run_dispatches(
        &self,
        limit: usize,
    ) -> Result<usize, String> {
        let pending = self.store.list_pending_run_dispatches(limit).await?;
        let mut published = 0usize;
        for run in pending {
            crate::run_dispatch_queue::enqueue_run_dispatch(
                &self.task_queue_topology,
                run.id.as_str(),
            )
            .await?;
            self.store
                .acknowledge_run_dispatch_event(run.id.as_str())
                .await?;
            published += 1;
        }
        Ok(published)
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
            crate::worker_control_queue::publish_run_terminal_event(
                &self.task_queue_topology,
                &run,
                &subscription,
            )
            .await?;
            self.store
                .acknowledge_run_terminal_subscription(subscription.id.as_str())
                .await?;
            published += 1;
        }
        Ok(published)
    }

    pub(crate) async fn enqueue_queued_runs_for_tasks(
        &self,
        task_ids: &[String],
    ) -> Result<usize, String> {
        if self.task_queue_topology.run_dispatch_mode != TaskQueueMode::RabbitMq {
            return Ok(0);
        }
        let mut enqueued = 0usize;
        for task_id in task_ids {
            let runs = self.store.list_runs(Some(task_id.as_str())).await?;
            for run in runs {
                if run.status != TaskRunStatus::Queued || run.dispatch_paused {
                    continue;
                }
                if self.enqueue_run_dispatch_if_needed(&run).await? {
                    enqueued += 1;
                }
            }
        }
        Ok(enqueued)
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

    #[test]
    fn claim_expiry_grace_is_a_small_clock_skew_window() {
        assert_eq!(
            worker_claim_expiry_grace(Duration::from_secs(120)),
            Duration::from_secs(12)
        );
    }

    #[test]
    fn short_claim_ttl_still_gets_minimum_expiry_grace() {
        assert_eq!(
            worker_claim_expiry_grace(Duration::from_secs(30)),
            MIN_WORKER_CLAIM_EXPIRY_GRACE
        );
    }

    #[test]
    fn long_claim_ttl_caps_expiry_grace() {
        assert_eq!(
            worker_claim_expiry_grace(Duration::from_secs(600)),
            MAX_WORKER_CLAIM_EXPIRY_GRACE
        );
    }
}
