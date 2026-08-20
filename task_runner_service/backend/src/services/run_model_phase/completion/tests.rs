// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::ask_user_prompt_service::AskUserPromptService;
use crate::config::{AppConfig, StoreMode};
use crate::models::{CreateTaskRequest, TaskClosureState, TaskManagerScope};
use crate::services::TaskService;
use crate::store::AppStore;
use chatos_ai_runtime::{
    AiTurnStatus, TaskAcceptanceEvidence, TaskExecutionOutcome, TaskExecutionOutcomeStatus,
};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

fn terminal_blocked_tasks_summary(tasks: &[TaskRecord]) -> String {
    let mut reason_groups = Vec::<(String, Vec<String>)>::new();
    for task in tasks {
        let raw_reason = task
            .task_tool_state
            .closure_reason
            .as_deref()
            .or(task.task_tool_state.blocker_reason.as_deref())
            .unwrap_or("未提供阻塞原因");
        let reason = user_facing_terminal_block_reason(raw_reason);
        if let Some((_, titles)) = reason_groups
            .iter_mut()
            .find(|(existing, _)| existing == &reason)
        {
            titles.push(task.title.trim().to_string());
        } else {
            reason_groups.push((reason, vec![task.title.trim().to_string()]));
        }
    }

    if let [(reason, titles)] = reason_groups.as_slice() {
        return format!(
            "本次运行未完成：{} 个必需步骤未完成。共同原因：{}。未完成步骤：{}",
            tasks.len(),
            reason,
            summarized_blocked_titles(titles)
        );
    }

    let details = reason_groups
        .iter()
        .take(3)
        .map(|(reason, titles)| {
            format!(
                "{}（{} 个步骤：{}）",
                reason,
                titles.len(),
                summarized_blocked_titles(titles)
            )
        })
        .collect::<Vec<_>>()
        .join("；");
    format!(
        "本次运行未完成：{} 个必需步骤未完成。阻塞明细：{}",
        tasks.len(),
        details
    )
}

fn user_facing_terminal_block_reason(reason: &str) -> String {
    if reason_looks_like_internal_tool_availability_blocker(reason) {
        return "当前运行没有可继续使用的仓库读取或终端能力，模型未完成必要实现或验证；这属于执行收口原因，不代表业务任务已完成或沙箱初始化失败"
            .to_string();
    }
    if reason.contains("连续两次") && reason.contains("校验没有状态进展") {
        return "模型结束前仍未完成必要的代码修改或验证，系统连续两次检查都没有看到实际进展"
            .to_string();
    }
    if reason.contains("达到") && reason.contains("最大收口轮次") {
        return "模型在允许的收尾轮次内仍未完成必要步骤".to_string();
    }
    reason.trim().to_string()
}

fn reason_looks_like_internal_tool_availability_blocker(reason: &str) -> bool {
    let normalized = reason.to_ascii_lowercase();
    let mentions_tool_surface = [
        "仓库读取",
        "读取能力",
        "文件读取",
        "源码读取",
        "未读取",
        "读取工具",
        "读文件工具",
        "代码读取",
        "真实文件",
        "schema",
        "测试执行能力",
        "执行能力",
        "验证能力",
        "运行测试",
        "终端",
        "执行工具",
        "工具清单",
        "read tool",
        "read-file",
        "read_file",
        "terminal",
        "tool surface",
        "tool list",
    ]
    .iter()
    .any(|needle| normalized.contains(needle));
    let mentions_availability = [
        "未暴露",
        "未能",
        "没有",
        "缺少",
        "无法",
        "不能",
        "不可用",
        "临时限制",
        "临时不可用",
        "被限制",
        "not exposed",
        "unavailable",
        "temporarily unavailable",
        "disabled",
        "rate-limited",
        "rate limited",
    ]
    .iter()
    .any(|needle| normalized.contains(needle));
    mentions_tool_surface && mentions_availability
}

fn summarized_blocked_titles(titles: &[String]) -> String {
    let shown = titles
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>()
        .join("、");
    if titles.len() > 5 {
        format!("{} 等 {} 个", shown, titles.len())
    } else {
        shown
    }
}

fn test_config() -> AppConfig {
    AppConfig {
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        otlp_endpoint: "http://127.0.0.1:4317".to_string(),
        otlp_trace_sample_ratio: 0.0,
        otlp_export_timeout: Duration::from_secs(1),
        role: crate::config::TaskRunnerRole::All,
        store_mode: StoreMode::Memory,
        database_url: "memory://run-completion-test".to_string(),
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

#[test]
fn structured_summary_exposes_supply_chain_audit_evidence() {
    let outcome = TaskExecutionOutcome::succeeded(
        "implemented",
        vec![
            "tests passed".to_string(),
            "Node.js supply-chain audit status: passed; baseline baseline-2026-08; command `npm audit --audit-level=high --json` exited 0; vulnerabilities total=0, high=0, critical=0".to_string(),
        ],
    );
    let summary = structured_outcome_result_summary(
        &outcome,
        false,
        &crate::services::path_redaction::WorkspacePathRedactor::for_workspace(".", "."),
    );

    assert!(summary.contains("供应链审计"));
    assert!(summary.contains("critical=0"));
}

async fn test_services() -> (TaskService, RunService) {
    let config = test_config();
    let store = AppStore::new(&config).await.expect("store");
    let task_service = TaskService::new(config.clone(), store.clone());
    let run_service = RunService::new(config, store.clone(), AskUserPromptService::new(store));
    (task_service, run_service)
}

#[tokio::test]
async fn runtime_cancel_signal_cancels_registered_token() {
    let (_, run_service) = test_services().await;
    let token = tokio_util::sync::CancellationToken::new();
    run_service.register_runtime_abort_token("run-registered", token.clone());

    run_service.signal_runtime_cancel("run-registered");

    assert!(token.is_cancelled());
}

#[tokio::test]
async fn runtime_abort_registration_observes_early_cancel_signal() {
    let (_, run_service) = test_services().await;
    run_service.signal_runtime_cancel("run-before-registration");
    let token = tokio_util::sync::CancellationToken::new();

    run_service.register_runtime_abort_token("run-before-registration", token.clone());

    assert!(token.is_cancelled());
}

async fn create_task(service: &TaskService, title: &str, status: TaskStatus) -> TaskRecord {
    service
        .create_task(
            CreateTaskRequest {
                title: title.to_string(),
                description: None,
                objective: format!("do {title}"),
                input_payload: None,
                status: Some(status),
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
            },
            None,
            None,
        )
        .await
        .expect("create task")
}

fn run_record(task: &TaskRecord) -> TaskRunRecord {
    let now = now_rfc3339();
    TaskRunRecord {
        id: "run-1".to_string(),
        task_id: task.id.clone(),
        agent_run_id: None,
        agent_ordering_lane_key: None,
        agent_lane_seq: None,
        execution_lane_key: None,
        model_config_id: "model-1".to_string(),
        memory_thread_id: task.memory_thread_id.clone(),
        status: TaskRunStatus::Running,
        model_phase_status: crate::models::ModelPhaseStatus::Running,
        started_at: Some(now.clone()),
        finished_at: None,
        input_snapshot: json!({}),
        effective_tools: Default::default(),
        workspace_execution: None,
        mcp_runtime_session_ref: None,
        context_snapshot: None,
        result_summary: None,
        error_message: None,
        usage: None,
        report: None,
        cancel_requested: false,
        cancel_event_pending: false,
        dispatch_paused: false,
        dispatch_event_pending: false,
        post_process_event_pending: false,
        post_process_event_enqueued: false,
        post_process_completed: false,
        post_process_dead_lettered: false,
        post_process_attempt_count: 0,
        post_process_last_error: None,
        memory_summary_processed: false,
        chatos_followup_processed: false,
        summary_job_run_id: None,
        worker_id: None,
        claim_token: None,
        claim_until: None,
        attempt: 0,
        attempts: Vec::new(),
        chatos_started_callback_delivery: None,
        chatos_callback_delivery: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

#[tokio::test]
async fn completed_run_persists_success_when_report_completed() {
    let (task_service, run_service) = test_services().await;
    let parent = create_task(&task_service, "parent", TaskStatus::Ready).await;

    let mut run = run_record(&parent);
    run_service
        .store
        .save_run(run.clone())
        .await
        .expect("save run");
    let report = TaskRunReport {
        task_id: parent.id.clone(),
        run_id: run.id.clone(),
        model_config_id: Some(run.model_config_id.clone()),
        status: AiTurnStatus::Completed,
        execution_outcome: Some(TaskExecutionOutcome::succeeded(
            "done",
            vec!["focused test passed".to_string()],
        )),
        content: Some("done".to_string()),
        reasoning: None,
        error: None,
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        usage: None,
        response_id: None,
        completed_at: now_rfc3339(),
    };

    run_service
        .finalize_model_phase(&parent, &mut run, report, ".")
        .await;

    let saved_run = run_service
        .store
        .get_run(run.id.as_str())
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(saved_run.status, TaskRunStatus::Succeeded);

    let saved_parent = task_service
        .get_task(parent.id.as_str())
        .await
        .expect("get parent")
        .expect("parent");
    assert_eq!(saved_parent.status, TaskStatus::Succeeded);
}

#[tokio::test]
async fn successful_write_run_waits_for_workspace_integration_before_business_success() {
    let (task_service, run_service) = test_services().await;
    let task = create_task(&task_service, "write task", TaskStatus::Ready).await;
    let mut run = run_record(&task);
    run.workspace_execution = Some(
        serde_json::from_value(json!({
            "status": "ready",
            "branch_target": {
                "kind": "run",
                "branch_id": "project:run-1",
                "branch_ref": "chatos/runs/run-1",
                "base_branch": "chatos/executions/group-1",
                "base_commit": "1111111111111111111111111111111111111111"
            },
            "execution_group_id": "group-1",
            "execution_branch_ref": "chatos/executions/group-1",
            "execution_base_commit": "1111111111111111111111111111111111111111",
            "integration_status": "pending"
        }))
        .expect("workspace execution"),
    );
    run_service
        .store
        .save_run(run.clone())
        .await
        .expect("save run");
    let report = TaskRunReport {
        task_id: task.id.clone(),
        run_id: run.id.clone(),
        model_config_id: Some(run.model_config_id.clone()),
        status: AiTurnStatus::Completed,
        execution_outcome: Some(TaskExecutionOutcome::succeeded(
            "done",
            vec!["write verified".to_string()],
        )),
        content: Some("done".to_string()),
        reasoning: None,
        error: None,
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        usage: None,
        response_id: None,
        completed_at: now_rfc3339(),
    };

    run_service
        .finalize_model_phase(&task, &mut run, report, ".")
        .await;

    let saved_run = run_service
        .store
        .get_run(run.id.as_str())
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(saved_run.status, TaskRunStatus::Running);
    assert_eq!(
        saved_run.model_phase_status,
        crate::models::ModelPhaseStatus::Succeeded
    );
    assert_eq!(
        saved_run
            .workspace_execution
            .as_ref()
            .map(|workspace| workspace.integration_status),
        Some(crate::models::WorkspaceIntegrationStatus::Pending)
    );
    assert!(saved_run.post_process_event_pending || saved_run.post_process_event_enqueued);
    assert!(saved_run.chatos_callback_delivery.is_none());
    let saved_task = task_service
        .get_task(task.id.as_str())
        .await
        .expect("get task")
        .expect("task");
    assert_eq!(saved_task.status, TaskStatus::Running);
}

#[tokio::test]
async fn cancel_run_waiting_for_workspace_integration_finishes_as_cancelled() {
    let (task_service, run_service) = test_services().await;
    let mut task = create_task(&task_service, "write task", TaskStatus::Ready).await;
    task.status = TaskStatus::Running;
    run_service
        .store
        .save_task(task.clone())
        .await
        .expect("save running task");
    let mut run = run_record(&task);
    run.model_phase_status = crate::models::ModelPhaseStatus::Succeeded;
    run.workspace_execution = Some(
        serde_json::from_value(json!({
            "status": "ready",
            "branch_target": {
                "kind": "run",
                "branch_id": "project:run-1",
                "branch_ref": "chatos/runs/run-1",
                "base_branch": "chatos/executions/group-1",
                "base_commit": "1111111111111111111111111111111111111111"
            },
            "execution_group_id": "group-1",
            "execution_branch_ref": "chatos/executions/group-1",
            "execution_base_commit": "1111111111111111111111111111111111111111",
            "integration_status": "pending"
        }))
        .expect("workspace execution"),
    );
    run_service
        .store
        .save_run(run.clone())
        .await
        .expect("save run");

    let cancelled = run_service
        .cancel_run(run.id.as_str())
        .await
        .expect("cancel run")
        .expect("run");

    assert_eq!(cancelled.status, TaskRunStatus::Cancelled);
    assert_eq!(
        cancelled.model_phase_status,
        crate::models::ModelPhaseStatus::Cancelled
    );
    assert!(!cancelled.cancel_requested);
    assert_eq!(
        cancelled
            .workspace_execution
            .as_ref()
            .map(|workspace| workspace.integration_status),
        Some(crate::models::WorkspaceIntegrationStatus::NotRequired)
    );
    let saved_task = task_service
        .get_task(task.id.as_str())
        .await
        .expect("get task")
        .expect("task");
    assert_eq!(saved_task.status, TaskStatus::Cancelled);
}

#[tokio::test]
async fn completed_runtime_without_structured_outcome_fails_closed() {
    let (task_service, run_service) = test_services().await;
    let task = create_task(&task_service, "missing outcome", TaskStatus::Ready).await;
    let mut run = run_record(&task);
    run_service
        .store
        .save_run(run.clone())
        .await
        .expect("save run");
    let report = TaskRunReport {
        task_id: task.id.clone(),
        run_id: run.id.clone(),
        model_config_id: Some(run.model_config_id.clone()),
        status: AiTurnStatus::Completed,
        execution_outcome: None,
        content: Some("claimed success".to_string()),
        reasoning: None,
        error: None,
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        usage: None,
        response_id: None,
        completed_at: now_rfc3339(),
    };

    run_service
        .finalize_model_phase(&task, &mut run, report, ".")
        .await;

    let saved_run = run_service
        .store
        .get_run(run.id.as_str())
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(saved_run.status, TaskRunStatus::Failed);
    assert!(saved_run
        .error_message
        .as_deref()
        .is_some_and(|error| error.contains("structured execution outcome")));
    assert!(saved_run
        .result_summary
        .as_deref()
        .is_some_and(|summary| summary.contains("structured execution outcome")));
    let saved_task = task_service
        .get_task(task.id.as_str())
        .await
        .expect("get task")
        .expect("task");
    assert_eq!(saved_task.status, TaskStatus::Failed);
}

#[tokio::test]
async fn structured_blocked_outcome_persists_blocked_terminal_state() {
    let (task_service, run_service) = test_services().await;
    let task = create_task(&task_service, "blocked outcome", TaskStatus::Ready).await;
    let mut run = run_record(&task);
    run_service
        .store
        .save_run(run.clone())
        .await
        .expect("save run");
    let report = TaskRunReport {
        task_id: task.id.clone(),
        run_id: run.id.clone(),
        model_config_id: Some(run.model_config_id.clone()),
        status: AiTurnStatus::Completed,
        execution_outcome: Some(TaskExecutionOutcome {
            status: TaskExecutionOutcomeStatus::Blocked,
            summary: "implementation could not be verified".to_string(),
            blocking_reason: Some("required upstream service is unavailable".to_string()),
            unmet_acceptance_criteria: vec!["integration verification passes".to_string()],
            verification_evidence: vec!["health request returned connection refused".to_string()],
            acceptance_evidence: Vec::new(),
            referenced_paths: Vec::new(),
            referenced_endpoints: Vec::new(),
        }),
        content: Some("work stopped before verification".to_string()),
        reasoning: None,
        error: None,
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        usage: None,
        response_id: None,
        completed_at: now_rfc3339(),
    };

    run_service
        .finalize_model_phase(&task, &mut run, report, ".")
        .await;

    let saved_run = run_service
        .store
        .get_run(run.id.as_str())
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(saved_run.status, TaskRunStatus::Blocked);
    assert_eq!(
        saved_run.error_message.as_deref(),
        Some("required upstream service is unavailable")
    );
    let saved_task = task_service
        .get_task(task.id.as_str())
        .await
        .expect("get task")
        .expect("task");
    assert_eq!(saved_task.status, TaskStatus::Blocked);
}

#[tokio::test]
async fn completed_report_ignores_legacy_terminal_checklist_blocker() {
    let (task_service, run_service) = test_services().await;
    let parent = create_task(&task_service, "parent", TaskStatus::Ready).await;
    let mut run = run_record(&parent);
    run_service
        .store
        .save_run(run.clone())
        .await
        .expect("save run");

    let now = now_rfc3339();
    let mut child = parent.clone();
    child.id = "blocked-child".to_string();
    child.title = "blocked child".to_string();
    child.status = TaskStatus::Blocked;
    child.parent_task_id = Some(parent.id.clone());
    child.source_run_id = Some(run.id.clone());
    child.last_run_id = None;
    child.task_tool_state.manager_scope = Some(TaskManagerScope::RunChecklist);
    child.task_tool_state.task_session_id = Some(run.id.clone());
    child.task_tool_state.required_for_parent_completion = Some(true);
    child.task_tool_state.closure_state = Some(TaskClosureState::BlockedTerminal);
    child.task_tool_state.closure_reason = Some("upstream API unavailable".to_string());
    child.task_tool_state.lifecycle_updated_at = Some(now.clone());
    child.created_at = now.clone();
    child.updated_at = now;
    run_service
        .store
        .save_task(child)
        .await
        .expect("save blocked child");

    let report = TaskRunReport {
        task_id: parent.id.clone(),
        run_id: run.id.clone(),
        model_config_id: Some(run.model_config_id.clone()),
        status: AiTurnStatus::Completed,
        execution_outcome: Some(TaskExecutionOutcome::succeeded(
            "done",
            vec!["focused test passed".to_string()],
        )),
        content: Some("done".to_string()),
        reasoning: None,
        error: None,
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        usage: None,
        response_id: None,
        completed_at: now_rfc3339(),
    };

    run_service
        .finalize_model_phase(&parent, &mut run, report, ".")
        .await;

    let saved_run = run_service
        .store
        .get_run(run.id.as_str())
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(saved_run.status, TaskRunStatus::Succeeded);
    assert_eq!(saved_run.error_message, None);
    let saved_parent = task_service
        .get_task(parent.id.as_str())
        .await
        .expect("get parent")
        .expect("parent");
    assert_eq!(saved_parent.status, TaskStatus::Succeeded);
    let saved_child = task_service
        .get_task("blocked-child")
        .await
        .expect("get child")
        .expect("child");
    assert_eq!(saved_child.status, TaskStatus::Blocked);
    assert_eq!(
        saved_child.task_tool_state.closure_state,
        Some(TaskClosureState::BlockedTerminal)
    );
}

#[tokio::test]
async fn completed_report_ignores_legacy_required_open_checklist() {
    let (task_service, run_service) = test_services().await;
    let parent = create_task(&task_service, "parent", TaskStatus::Ready).await;
    let mut run = run_record(&parent);
    run_service
        .store
        .save_run(run.clone())
        .await
        .expect("save run");

    let now = now_rfc3339();
    let mut child = parent.clone();
    child.id = "forgotten-child".to_string();
    child.title = "forgotten child".to_string();
    child.status = TaskStatus::Ready;
    child.parent_task_id = Some(parent.id.clone());
    child.source_run_id = Some(run.id.clone());
    child.last_run_id = None;
    child.task_tool_state.manager_scope = Some(TaskManagerScope::RunChecklist);
    child.task_tool_state.task_session_id = Some(run.id.clone());
    child.task_tool_state.required_for_parent_completion = Some(true);
    child.task_tool_state.closure_state = Some(TaskClosureState::Open);
    child.task_tool_state.lifecycle_updated_at = Some(now.clone());
    child.created_at = now.clone();
    child.updated_at = now;
    run_service
        .store
        .save_task(child.clone())
        .await
        .expect("save forgotten child");

    let report = TaskRunReport {
        task_id: parent.id.clone(),
        run_id: run.id.clone(),
        model_config_id: Some(run.model_config_id.clone()),
        status: AiTurnStatus::Completed,
        execution_outcome: Some(TaskExecutionOutcome::succeeded(
            "done",
            vec!["focused test passed".to_string()],
        )),
        content: Some("done".to_string()),
        reasoning: None,
        error: None,
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        usage: None,
        response_id: None,
        completed_at: now_rfc3339(),
    };

    run_service
        .finalize_model_phase(&parent, &mut run, report, ".")
        .await;

    let saved_child = task_service
        .get_task(child.id.as_str())
        .await
        .expect("get child")
        .expect("child");
    assert_eq!(
        saved_child.task_tool_state.closure_state,
        Some(TaskClosureState::Open)
    );
    assert_eq!(saved_child.status, TaskStatus::Ready);
    let saved_run = run_service
        .store
        .get_run(run.id.as_str())
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(saved_run.status, TaskRunStatus::Succeeded);
    assert_eq!(saved_run.error_message, None);
    let saved_parent = task_service
        .get_task(parent.id.as_str())
        .await
        .expect("get parent")
        .expect("parent");
    assert_eq!(saved_parent.status, TaskStatus::Succeeded);
}

#[tokio::test]
async fn successful_run_preserves_legacy_optional_open_checklist() {
    let (task_service, run_service) = test_services().await;
    let parent = create_task(&task_service, "parent", TaskStatus::Ready).await;
    let mut run = run_record(&parent);
    run_service
        .store
        .save_run(run.clone())
        .await
        .expect("save run");

    let now = now_rfc3339();
    let mut child = parent.clone();
    child.id = "optional-child".to_string();
    child.title = "optional child".to_string();
    child.status = TaskStatus::Ready;
    child.parent_task_id = Some(parent.id.clone());
    child.source_run_id = Some(run.id.clone());
    child.last_run_id = None;
    child.task_tool_state.manager_scope = Some(TaskManagerScope::RunChecklist);
    child.task_tool_state.task_session_id = Some(run.id.clone());
    child.task_tool_state.required_for_parent_completion = Some(false);
    child.task_tool_state.closure_state = Some(TaskClosureState::Open);
    child.task_tool_state.lifecycle_updated_at = Some(now.clone());
    child.created_at = now.clone();
    child.updated_at = now;
    run_service
        .store
        .save_task(child.clone())
        .await
        .expect("save optional child");

    let report = TaskRunReport {
        task_id: parent.id.clone(),
        run_id: run.id.clone(),
        model_config_id: Some(run.model_config_id.clone()),
        status: AiTurnStatus::Completed,
        execution_outcome: Some(TaskExecutionOutcome::succeeded(
            "done",
            vec!["focused test passed".to_string()],
        )),
        content: Some("done".to_string()),
        reasoning: None,
        error: None,
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        usage: None,
        response_id: None,
        completed_at: now_rfc3339(),
    };

    run_service
        .finalize_model_phase(&parent, &mut run, report, ".")
        .await;

    let saved_child = task_service
        .get_task(child.id.as_str())
        .await
        .expect("get child")
        .expect("child");
    assert_eq!(
        saved_child.task_tool_state.closure_state,
        Some(TaskClosureState::Open)
    );
    assert_eq!(saved_child.status, TaskStatus::Ready);
    let saved_run = run_service
        .store
        .get_run(run.id.as_str())
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(saved_run.status, TaskRunStatus::Succeeded);
}

#[tokio::test]
async fn aborted_report_does_not_downgrade_already_succeeded_task() {
    let (task_service, run_service) = test_services().await;
    let mut parent = create_task(&task_service, "parent", TaskStatus::Succeeded).await;
    parent.result_summary = Some("completed before abort".to_string());
    run_service
        .store
        .save_task(parent.clone())
        .await
        .expect("save succeeded parent");

    let mut run = run_record(&parent);
    run_service
        .store
        .save_run(run.clone())
        .await
        .expect("save run");
    let report = TaskRunReport {
        task_id: parent.id.clone(),
        run_id: run.id.clone(),
        model_config_id: Some(run.model_config_id.clone()),
        status: AiTurnStatus::Aborted,
        execution_outcome: None,
        content: None,
        reasoning: None,
        error: Some("aborted".to_string()),
        tool_calls: None,
        finish_reason: None,
        usage: None,
        response_id: None,
        completed_at: now_rfc3339(),
    };

    run_service
        .finalize_model_phase(&parent, &mut run, report, ".")
        .await;

    let saved_run = run_service
        .store
        .get_run(run.id.as_str())
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(saved_run.status, TaskRunStatus::Succeeded);
    assert_eq!(
        saved_run.result_summary.as_deref(),
        Some("completed before abort")
    );
    assert_eq!(saved_run.error_message, None);

    let saved_parent = task_service
        .get_task(parent.id.as_str())
        .await
        .expect("get parent")
        .expect("parent");
    assert_eq!(saved_parent.status, TaskStatus::Succeeded);
}

#[test]
fn execution_outcome_reference_validation_accepts_real_workspace_evidence() {
    let workspace = temporary_reference_workspace("valid");
    std::fs::create_dir_all(workspace.join("apps/api")).expect("create app directory");
    std::fs::write(workspace.join("apps/api/package.json"), "{}").expect("write referenced file");
    let mut outcome = TaskExecutionOutcome::succeeded(
        "implementation verified",
        vec!["cargo test passed".to_string()],
    );
    outcome.referenced_paths = vec!["apps/api".to_string(), "apps/api/package.json".to_string()];
    outcome.referenced_endpoints = vec!["http://127.0.0.1:4000/health".to_string()];

    validate_task_execution_outcome_references(&mut outcome).expect("valid references");

    std::fs::remove_dir_all(workspace).expect("remove workspace");
}

#[test]
fn execution_outcome_reference_validation_defers_existence_to_provider_but_rejects_escaping_paths()
{
    let workspace = temporary_reference_workspace("invalid-path");
    let mut outcome = TaskExecutionOutcome::succeeded(
        "implementation verified",
        vec!["cargo test passed".to_string()],
    );
    outcome.referenced_paths = vec!["missing.txt".to_string()];
    validate_task_execution_outcome_references(&mut outcome)
        .expect("MCP provider owns path existence validation");

    outcome.referenced_paths = vec!["../outside.txt".to_string()];
    let escaping = validate_task_execution_outcome_references(&mut outcome)
        .expect_err("planning references must not escape the workspace");
    assert!(escaping.contains("must stay inside the workspace"));

    std::fs::remove_dir_all(workspace).expect("remove workspace");
}

#[test]
fn execution_outcome_reference_validation_rejects_endpoint_credentials() {
    let workspace = temporary_reference_workspace("invalid-endpoint");
    let mut outcome = TaskExecutionOutcome::succeeded(
        "implementation verified",
        vec!["health check passed".to_string()],
    );
    outcome.referenced_endpoints = vec!["https://admin:secret@example.com/health".to_string()];

    let error = validate_task_execution_outcome_references(&mut outcome)
        .expect_err("endpoint credentials must fail");
    assert!(error.contains("must not contain credentials"));

    std::fs::remove_dir_all(workspace).expect("remove workspace");
}

#[test]
fn invalid_evidence_receipts_are_dropped_without_changing_ai_reported_status() {
    let mut outcome = TaskExecutionOutcome::succeeded(
        "implementation verified",
        vec!["AI explicitly reported succeeded".to_string()],
    );
    outcome.referenced_paths = vec!["src/main.rs".to_string(), "../outside.txt".to_string()];
    outcome.referenced_endpoints = vec![
        "http://127.0.0.1:4000/health".to_string(),
        "https://admin:secret@example.com/health".to_string(),
    ];
    outcome.acceptance_evidence = vec![TaskAcceptanceEvidence {
        criterion: "service works".to_string(),
        evidence: vec!["reported complete".to_string()],
        referenced_paths: vec!["src/main.rs".to_string(), "../../secret".to_string()],
        commands: Vec::new(),
        tool_names: Vec::new(),
    }];

    sanitize_task_execution_outcome_references(&mut outcome);

    assert_eq!(outcome.status, TaskExecutionOutcomeStatus::Succeeded);
    assert_eq!(outcome.referenced_paths, ["src/main.rs"]);
    assert_eq!(
        outcome.referenced_endpoints,
        ["http://127.0.0.1:4000/health"]
    );
    assert_eq!(
        outcome.acceptance_evidence[0].referenced_paths,
        ["src/main.rs"]
    );
}

#[test]
fn execution_outcome_reference_validation_preserves_provider_verified_path() {
    let workspace = temporary_reference_workspace("single-character-typo");
    let migrations = workspace.join("src/server/database/migrations");
    std::fs::create_dir_all(&migrations).expect("create migrations directory");
    std::fs::write(migrations.join("0000_baseline.sql"), "select 1;").expect("write migration");
    let mut outcome = TaskExecutionOutcome::succeeded(
        "migration verified",
        vec!["migration command passed".to_string()],
    );
    outcome.referenced_paths = vec!["src/server/database/migrations/000_baseline.sql".to_string()];

    validate_task_execution_outcome_references(&mut outcome)
        .expect("Task Runner must not rewrite MCP provider paths");
    assert_eq!(
        outcome.referenced_paths,
        vec!["src/server/database/migrations/000_baseline.sql"]
    );

    std::fs::remove_dir_all(workspace).expect("remove workspace");
}

#[test]
fn execution_outcome_reference_validation_does_not_rewrite_provider_directories() {
    let workspace = temporary_reference_workspace("single-character-directory-typo");
    let seeds = workspace.join("db/seeds");
    std::fs::create_dir_all(&seeds).expect("create seeds directory");
    std::fs::write(seeds.join("tasks.sql"), "select 1;").expect("write seed file");
    let mut outcome =
        TaskExecutionOutcome::succeeded("seed verified", vec!["seed command passed".to_string()]);
    outcome.referenced_paths = vec!["db/seds/tasks.sql".to_string()];

    validate_task_execution_outcome_references(&mut outcome)
        .expect("Task Runner must not inspect provider directories");
    assert_eq!(outcome.referenced_paths, vec!["db/seds/tasks.sql"]);

    std::fs::remove_dir_all(workspace).expect("remove workspace");
}

#[test]
fn execution_outcome_reference_validation_does_not_inspect_ambiguous_provider_directories() {
    let workspace = temporary_reference_workspace("ambiguous-directory-typo");
    std::fs::create_dir_all(workspace.join("db/seeds")).expect("create seeds directory");
    std::fs::create_dir_all(workspace.join("db/sods")).expect("create sods directory");
    let mut outcome =
        TaskExecutionOutcome::succeeded("seed verified", vec!["seed command passed".to_string()]);
    outcome.referenced_paths = vec!["db/seds/tasks.sql".to_string()];

    validate_task_execution_outcome_references(&mut outcome)
        .expect("MCP provider owns path existence validation");

    std::fs::remove_dir_all(workspace).expect("remove workspace");
}

fn temporary_reference_workspace(label: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "chatos-outcome-reference-{label}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("create temporary workspace");
    path
}

#[tokio::test]
async fn terminal_blocked_summary_groups_repeated_internal_reason() {
    let (task_service, _) = test_services().await;
    let reason = "模型完成声明后，连续两次系统收口校验没有状态进展；仍有必需清单未完成，父任务不能标记为成功";
    let mut first = create_task(&task_service, "实现后端", TaskStatus::Blocked).await;
    first.task_tool_state.closure_reason = Some(reason.to_string());
    let mut second = create_task(&task_service, "执行验证", TaskStatus::Blocked).await;
    second.task_tool_state.closure_reason = Some(reason.to_string());

    let summary = terminal_blocked_tasks_summary(&[first, second]);

    assert!(summary.contains("本次运行未完成：2 个必需步骤未完成"));
    assert!(summary.contains("共同原因"));
    assert!(summary.contains("实现后端、执行验证"));
    assert!(!summary.contains("系统收口"));
    assert_eq!(summary.matches("连续两次检查").count(), 1);
}

#[tokio::test]
async fn terminal_blocked_summary_rewrites_internal_tool_availability_reason() {
    let (task_service, _) = test_services().await;
    let mut task = create_task(&task_service, "执行验证", TaskStatus::Blocked).await;
    task.task_tool_state.closure_reason =
        Some("当前运行未暴露仓库读取工具与终端执行工具，无法继续。".to_string());

    let summary = terminal_blocked_tasks_summary(&[task]);

    assert!(summary.contains("本次运行未完成：1 个必需步骤未完成"));
    assert!(summary.contains("不代表业务任务已完成或沙箱初始化失败"));
    assert!(!summary.contains("未暴露仓库读取工具"));
    assert!(!summary.contains("系统拦截"));
}
