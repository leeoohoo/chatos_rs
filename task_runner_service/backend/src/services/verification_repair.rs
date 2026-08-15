// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};
use tracing::info;
use uuid::Uuid;

use crate::models::{
    now_rfc3339, TaskClosureState, TaskListFilters, TaskRecord, TaskRunEventRecord, TaskRunRecord,
    TaskRunStatus, TaskScheduleConfig, TaskScheduleMode, TaskStatus,
};

use super::{project_management_api_client, RunService};

const VERIFICATION_REPAIR_MAX_ATTEMPTS: u32 = 2;
const REPAIR_CHAIN_ROLE_KEY: &str = "repair_chain_role";
const REPAIR_CHAIN_ROLE_REPAIR: &str = "repair";
const REPAIR_CHAIN_ROLE_REVERIFY: &str = "reverify";
const REPAIR_ORIGIN_RUN_ID_KEY: &str = "repair_origin_verification_run_id";
const REPAIR_CHAIN_CREATED_EVENT_TYPE: &str = "verification_repair_chain_created";

#[derive(Debug, Clone)]
struct VerificationRepairPlan {
    project_task_id: String,
    execution_group_id: String,
    owned_paths: Vec<String>,
    acceptance_criteria: Vec<String>,
    successful_implementation_prerequisites: Vec<TaskRecord>,
    repair_attempt: u32,
}

#[derive(Debug, Default)]
struct ExistingRepairChain {
    repair: Option<TaskRecord>,
    reverify: Option<TaskRecord>,
}

#[derive(Debug)]
struct PersistedVerificationRepairChain {
    repair: TaskRecord,
    reverify: TaskRecord,
    repair_created: bool,
    reverify_created: bool,
    superseded_changed: bool,
}

impl PersistedVerificationRepairChain {
    fn created_event_required(&self) -> bool {
        self.repair_created || self.reverify_created || self.superseded_changed
    }
}

impl RunService {
    pub(in crate::services) async fn ensure_verification_repair_chain(
        &self,
        verification: &TaskRecord,
        verification_run: &TaskRunRecord,
    ) -> Result<(), String> {
        let Some(plan) = self
            .build_verification_repair_plan(verification, verification_run)
            .await?
        else {
            return Ok(());
        };

        let persisted = self
            .persist_verification_repair_chain(verification, verification_run, &plan)
            .await?;
        let repair = &persisted.repair;
        let reverify = &persisted.reverify;

        self.sync_repair_chain_links(verification, verification_run, &plan, &repair, &reverify)
            .await?;

        let dispatched = self
            .dispatch_ready_chatos_async_tasks_for_source_task(&repair)
            .await?;
        let event_id = verification_repair_chain_event_id(verification_run.id.as_str());
        if persisted.created_event_required()
            || self
                .store
                .get_run_event(verification_run.id.as_str(), event_id.as_str())
                .await?
                .is_none()
        {
            let mut event = TaskRunEventRecord::new(
                verification_run.id.clone(),
                REPAIR_CHAIN_CREATED_EVENT_TYPE,
                Some(format!(
                    "验收未通过，已创建第 {} 次修复与重新验收任务",
                    plan.repair_attempt
                )),
                Some(json!({
                    "repair_task_id": repair.id,
                    "reverify_task_id": reverify.id,
                    "repair_attempt": plan.repair_attempt,
                    "owned_paths": plan.owned_paths,
                    "auto_started_run_ids": dispatched.iter().map(|run| run.id.as_str()).collect::<Vec<_>>(),
                })),
            );
            event.id = event_id;
            self.store.append_run_event(event).await?;
        }
        info!(
            verification_task_id = verification.id.as_str(),
            verification_run_id = verification_run.id.as_str(),
            repair_task_id = repair.id.as_str(),
            reverify_task_id = reverify.id.as_str(),
            repair_attempt = plan.repair_attempt,
            "created or recovered verification repair chain"
        );
        Ok(())
    }

    async fn persist_verification_repair_chain(
        &self,
        verification: &TaskRecord,
        verification_run: &TaskRunRecord,
        plan: &VerificationRepairPlan,
    ) -> Result<PersistedVerificationRepairChain, String> {
        let mut existing = self
            .find_existing_repair_chain(
                verification.project_id.as_str(),
                verification_run.id.as_str(),
            )
            .await?;
        let now = now_rfc3339();

        let (repair, repair_created) = match existing.repair.take() {
            Some(task) => (task, false),
            None => {
                let task = build_repair_task(verification, verification_run, plan, now.as_str());
                let task = self.store.save_task(task).await?;
                self.store
                    .set_task_prerequisites(task.id.as_str(), task.prerequisite_task_ids.clone())
                    .await?;
                (task, true)
            }
        };

        let (reverify, reverify_created) = match existing.reverify.take() {
            Some(task) => (task, false),
            None => {
                let task = build_reverify_task(
                    verification,
                    verification_run,
                    plan,
                    &repair,
                    now.as_str(),
                );
                let task = self.store.save_task(task).await?;
                self.store
                    .set_task_prerequisites(task.id.as_str(), task.prerequisite_task_ids.clone())
                    .await?;
                (task, true)
            }
        };

        let mut superseded = self
            .store
            .get_task(verification.id.as_str())
            .await?
            .unwrap_or_else(|| verification.clone());
        let superseded_changed =
            superseded.task_tool_state.superseded_by_task_id.as_deref() != Some(repair.id.as_str());
        if superseded_changed {
            superseded.status = TaskStatus::Cancelled;
            superseded.task_tool_state.superseded_by_task_id = Some(repair.id.clone());
            superseded.task_tool_state.replacement_task_ids =
                vec![repair.id.clone(), reverify.id.clone()];
            superseded.task_tool_state.closure_state = Some(TaskClosureState::Superseded);
            superseded.task_tool_state.closure_reason = Some(format!(
                "验收未通过，已自动进入第 {} 次 repair/reverify 闭环",
                plan.repair_attempt
            ));
            superseded.task_tool_state.lifecycle_updated_at = Some(now.clone());
            superseded.updated_at = now;
            self.store.save_task(superseded).await?;
        }

        Ok(PersistedVerificationRepairChain {
            repair,
            reverify,
            repair_created,
            reverify_created,
            superseded_changed,
        })
    }

    async fn build_verification_repair_plan(
        &self,
        verification: &TaskRecord,
        verification_run: &TaskRunRecord,
    ) -> Result<Option<VerificationRepairPlan>, String> {
        if verification_run.status != TaskRunStatus::Blocked
            || payload_string(verification, "task_role").as_deref() != Some("verification")
        {
            return Ok(None);
        }

        let previous_attempt = verification
            .task_tool_state
            .repair_attempt
            .max(payload_u64(verification, "repair_attempt").unwrap_or(0) as u32);
        if previous_attempt >= VERIFICATION_REPAIR_MAX_ATTEMPTS {
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    verification_run.id.clone(),
                    "verification_repair_limit_reached",
                    Some(format!(
                        "验收连续 {} 次未通过，已达到自动修复上限",
                        previous_attempt
                    )),
                    Some(json!({
                        "repair_attempt": previous_attempt,
                        "max_attempts": VERIFICATION_REPAIR_MAX_ATTEMPTS,
                    })),
                ))
                .await?;
            return Ok(None);
        }

        let prerequisite_ids = self
            .store
            .list_task_prerequisites(verification.id.as_str())
            .await?
            .into_iter()
            .map(|edge| edge.prerequisite_task_id)
            .collect::<Vec<_>>();
        let mut implementation_prerequisites = Vec::new();
        let mut owned_paths = BTreeSet::new();
        let mut has_write_capability = false;
        for prerequisite_id in prerequisite_ids {
            let Some(prerequisite) = self.store.get_task(prerequisite_id.as_str()).await? else {
                continue;
            };
            if prerequisite.status != TaskStatus::Succeeded
                || payload_string(&prerequisite, "task_role").as_deref() != Some("implementation")
            {
                continue;
            }
            has_write_capability |=
                prerequisite
                    .mcp_config
                    .enabled_builtin_kinds
                    .iter()
                    .any(|kind| {
                        chatos_mcp_runtime::builtin_kind_by_any(kind)
                            == Some(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite)
                    });
            for path in payload_strings(&prerequisite, "owned_paths") {
                owned_paths.insert(path);
            }
            implementation_prerequisites.push(prerequisite);
        }
        if implementation_prerequisites.is_empty()
            || !has_write_capability
            || owned_paths.is_empty()
        {
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    verification_run.id.clone(),
                    "verification_repair_not_applicable",
                    Some(
                        "验收任务没有可复用的成功实施前置任务、写能力或文件所有权，未自动创建修复任务"
                            .to_string(),
                    ),
                    None,
                ))
                .await?;
            return Ok(None);
        }

        let project_task_id = payload_string(verification, "project_task_id")
            .ok_or_else(|| "verification task is missing project_task_id".to_string())?;
        let execution_group_id = payload_string(verification, "execution_group_id")
            .or_else(|| {
                verification_run
                    .workspace_execution
                    .as_ref()
                    .and_then(|workspace| workspace.execution_group_id.clone())
            })
            .ok_or_else(|| "verification task is missing execution_group_id".to_string())?;
        let acceptance_criteria = payload_strings(verification, "acceptance_criteria");
        if acceptance_criteria.is_empty() {
            return Err("verification task is missing acceptance_criteria".to_string());
        }

        Ok(Some(VerificationRepairPlan {
            project_task_id,
            execution_group_id,
            owned_paths: owned_paths.into_iter().collect(),
            acceptance_criteria,
            successful_implementation_prerequisites: implementation_prerequisites,
            repair_attempt: previous_attempt + 1,
        }))
    }

    async fn find_existing_repair_chain(
        &self,
        project_id: &str,
        verification_run_id: &str,
    ) -> Result<ExistingRepairChain, String> {
        let tasks = self
            .store
            .list_tasks_filtered(&TaskListFilters {
                project_id: Some(project_id.to_string()),
                include_subtasks: Some(false),
                ..TaskListFilters::default()
            })
            .await?;
        let mut chain = ExistingRepairChain::default();
        for task in tasks {
            if payload_string(&task, REPAIR_ORIGIN_RUN_ID_KEY).as_deref()
                != Some(verification_run_id)
            {
                continue;
            }
            match payload_string(&task, REPAIR_CHAIN_ROLE_KEY).as_deref() {
                Some(REPAIR_CHAIN_ROLE_REPAIR) => chain.repair = Some(task),
                Some(REPAIR_CHAIN_ROLE_REVERIFY) => chain.reverify = Some(task),
                _ => {}
            }
        }
        Ok(chain)
    }

    async fn sync_repair_chain_links(
        &self,
        verification: &TaskRecord,
        verification_run: &TaskRunRecord,
        plan: &VerificationRepairPlan,
        repair: &TaskRecord,
        reverify: &TaskRecord,
    ) -> Result<(), String> {
        let common = |task: &TaskRecord, supersedes_task_runner_task_ids: Vec<String>| {
            project_management_api_client::SyncTaskRunnerWorkItemStatusRequest {
                task_runner_task_id: task.id.clone(),
                task_runner_run_id: None,
                task_runner_status: Some("ready".to_string()),
                execution_group_id: Some(plan.execution_group_id.clone()),
                last_callback_event: Some("task.repair_planned".to_string()),
                last_callback_at: Some(now_rfc3339()),
                last_error_message: verification_run.error_message.clone(),
                source_session_id: verification.source_session_id.clone(),
                source_user_message_id: verification.source_user_message_id.clone(),
                supersedes_task_runner_task_ids,
            }
        };
        project_management_api_client::sync_work_item_task_runner_status(
            &self.config,
            plan.project_task_id.as_str(),
            &common(repair, vec![verification.id.clone()]),
        )
        .await?;
        project_management_api_client::sync_work_item_task_runner_status(
            &self.config,
            plan.project_task_id.as_str(),
            &common(reverify, Vec::new()),
        )
        .await?;
        Ok(())
    }
}

fn build_repair_task(
    verification: &TaskRecord,
    verification_run: &TaskRunRecord,
    plan: &VerificationRepairPlan,
    now: &str,
) -> TaskRecord {
    let mut repair = plan.successful_implementation_prerequisites[0].clone();
    let repair_id = Uuid::new_v4().to_string();
    repair.id = repair_id.clone();
    repair.title = format!("修复：{}", verification.title);
    repair.description = Some(format!(
        "根据验收 Run {} 的失败证据修复实现，并重新完成全部硬验收标准。",
        verification_run.id
    ));
    repair.objective = format!(
        "修复验收任务“{}”发现的问题。阻塞原因：{}。修复后运行与以下验收标准对应的测试：{}",
        verification.title,
        verification_run
            .error_message
            .as_deref()
            .unwrap_or("验收未通过"),
        plan.acceptance_criteria.join("；")
    );
    repair.input_payload = Some(repair_payload(
        verification,
        verification_run,
        plan,
        REPAIR_CHAIN_ROLE_REPAIR,
        "implementation",
        plan.owned_paths.as_slice(),
    ));
    repair.status = TaskStatus::Ready;
    repair.priority = verification.priority.max(repair.priority);
    repair.default_model_config_id = verification
        .default_model_config_id
        .clone()
        .or(repair.default_model_config_id);
    repair.memory_thread_id = format!("task-{repair_id}");
    repair.result_summary = None;
    repair.process_log = None;
    repair.last_run_id = None;
    repair.schedule = contact_async_schedule(now);
    repair.parent_task_id = verification.parent_task_id.clone();
    repair.source_run_id = verification.source_run_id.clone();
    repair.source_session_id = verification.source_session_id.clone();
    repair.source_turn_id = verification.source_turn_id.clone();
    repair.source_user_message_id = verification.source_user_message_id.clone();
    repair.prerequisite_task_ids = plan
        .successful_implementation_prerequisites
        .iter()
        .map(|task| task.id.clone())
        .collect();
    repair.task_tool_state = Default::default();
    repair.task_tool_state.repair_origin_verification_run_id = Some(verification_run.id.clone());
    repair.task_tool_state.repair_attempt = plan.repair_attempt;
    repair.task_tool_state.idempotency_key = Some(format!(
        "verification-repair:{}:repair",
        verification_run.id
    ));
    merge_implementation_capabilities(&mut repair, plan);
    repair.created_at = now.to_string();
    repair.updated_at = now.to_string();
    repair.deleted_at = None;
    repair
}

fn build_reverify_task(
    verification: &TaskRecord,
    verification_run: &TaskRunRecord,
    plan: &VerificationRepairPlan,
    repair: &TaskRecord,
    now: &str,
) -> TaskRecord {
    let mut reverify = verification.clone();
    let reverify_id = Uuid::new_v4().to_string();
    reverify.id = reverify_id.clone();
    reverify.title = format!("重新验收：{}", verification.title);
    reverify.description = Some(format!(
        "在修复任务 {} 集成成功后重新执行原验收。",
        repair.id
    ));
    reverify.objective = format!(
        "重新验收修复后的实现，逐条验证原始硬验收标准：{}",
        plan.acceptance_criteria.join("；")
    );
    reverify.input_payload = Some(repair_payload(
        verification,
        verification_run,
        plan,
        REPAIR_CHAIN_ROLE_REVERIFY,
        "verification",
        &[],
    ));
    reverify.status = TaskStatus::Ready;
    reverify.memory_thread_id = format!("task-{reverify_id}");
    reverify.result_summary = None;
    reverify.process_log = None;
    reverify.last_run_id = None;
    reverify.schedule = contact_async_schedule(now);
    reverify.prerequisite_task_ids = vec![repair.id.clone()];
    reverify.task_tool_state = Default::default();
    reverify.task_tool_state.repair_origin_verification_run_id = Some(verification_run.id.clone());
    reverify.task_tool_state.repair_attempt = plan.repair_attempt;
    reverify.task_tool_state.idempotency_key = Some(format!(
        "verification-repair:{}:reverify",
        verification_run.id
    ));
    reverify.mcp_config.enabled_builtin_kinds.retain(|kind| {
        chatos_mcp_runtime::builtin_kind_by_any(kind)
            != Some(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite)
    });
    reverify.created_at = now.to_string();
    reverify.updated_at = now.to_string();
    reverify.deleted_at = None;
    reverify
}

fn repair_payload(
    verification: &TaskRecord,
    verification_run: &TaskRunRecord,
    plan: &VerificationRepairPlan,
    chain_role: &str,
    task_role: &str,
    owned_paths: &[String],
) -> Value {
    let mut payload = verification
        .input_payload
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);
    payload.insert(
        "task_role".to_string(),
        Value::String(task_role.to_string()),
    );
    payload.insert(
        "owned_paths".to_string(),
        Value::Array(owned_paths.iter().cloned().map(Value::String).collect()),
    );
    payload.insert(
        "acceptance_criteria".to_string(),
        Value::Array(
            plan.acceptance_criteria
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    payload.insert(
        "project_task_id".to_string(),
        Value::String(plan.project_task_id.clone()),
    );
    payload.insert(
        "execution_group_id".to_string(),
        Value::String(plan.execution_group_id.clone()),
    );
    payload.insert(
        REPAIR_CHAIN_ROLE_KEY.to_string(),
        Value::String(chain_role.to_string()),
    );
    payload.insert(
        REPAIR_ORIGIN_RUN_ID_KEY.to_string(),
        Value::String(verification_run.id.clone()),
    );
    payload.insert(
        "repair_origin_verification_task_id".to_string(),
        Value::String(verification.id.clone()),
    );
    payload.insert(
        "repair_attempt".to_string(),
        Value::from(plan.repair_attempt),
    );
    payload.insert(
        "repair_origin_blocking_reason".to_string(),
        verification_run
            .error_message
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    payload.insert(
        "repair_origin_verification_report".to_string(),
        verification_run.report.clone().unwrap_or(Value::Null),
    );
    Value::Object(payload)
}

fn merge_implementation_capabilities(repair: &mut TaskRecord, plan: &VerificationRepairPlan) {
    let mut builtin_kinds = BTreeSet::new();
    let mut external_ids = BTreeSet::new();
    for prerequisite in &plan.successful_implementation_prerequisites {
        builtin_kinds.extend(
            prerequisite
                .mcp_config
                .enabled_builtin_kinds
                .iter()
                .cloned(),
        );
        external_ids.extend(
            prerequisite
                .mcp_config
                .external_mcp_config_ids
                .iter()
                .cloned(),
        );
        repair.mcp_config.requires_execution |= prerequisite.mcp_config.requires_execution;
        repair.mcp_config.workspace_changes_required |=
            prerequisite.mcp_config.workspace_changes_required;
    }
    repair.mcp_config.enabled_builtin_kinds = builtin_kinds.into_iter().collect();
    repair.mcp_config.external_mcp_config_ids = external_ids.into_iter().collect();
}

fn contact_async_schedule(now: &str) -> TaskScheduleConfig {
    TaskScheduleConfig {
        mode: TaskScheduleMode::ContactAsync,
        run_at: Some(now.to_string()),
        interval_seconds: None,
        next_run_at: None,
        last_scheduled_at: None,
    }
}

fn payload_string(task: &TaskRecord, key: &str) -> Option<String> {
    task.input_payload
        .as_ref()?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn payload_strings(task: &TaskRecord, key: &str) -> Vec<String> {
    task.input_payload
        .as_ref()
        .and_then(|payload| payload.get(key))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn payload_u64(task: &TaskRecord, key: &str) -> Option<u64> {
    task.input_payload.as_ref()?.get(key)?.as_u64()
}

fn verification_repair_chain_event_id(verification_run_id: &str) -> String {
    format!("verification_repair_chain_created:{verification_run_id}")
}

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
            user_service_request_timeout: Duration::from_millis(1000),
            project_service_base_url: None,
            project_service_internal_base_url: None,
            project_service_internal_http_client: reqwest::Client::new(),
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(1000),
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
            project_id: "project-1".to_string(),
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
            prerequisite_task_ids: Vec::new(),
            task_tool_state: TaskToolState::default(),
            plugin_config: TaskPluginConfig::default(),
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
                project_id: Some(verification.project_id.clone()),
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
