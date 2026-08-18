// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn test_store() -> InMemoryStore {
    let (sender, _) = broadcast::channel(16);
    InMemoryStore::new(sender)
}

fn queued_run() -> TaskRunRecord {
    let now = now_rfc3339();
    TaskRunRecord {
        id: "run-1".to_string(),
        task_id: "task-1".to_string(),
        agent_run_id: None,
        agent_ordering_lane_key: None,
        agent_lane_seq: None,
        execution_lane_key: None,
        model_config_id: "model-1".to_string(),
        memory_thread_id: "thread-1".to_string(),
        status: TaskRunStatus::Queued,
        model_phase_status: crate::models::ModelPhaseStatus::Pending,
        started_at: None,
        finished_at: None,
        input_snapshot: serde_json::json!({}),
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
        dispatch_event_pending: true,
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
        chatos_callback_delivery: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

#[test]
fn execution_stats_count_runs_and_pending_outboxes_without_cloning_records() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");

    let mut running = queued_run();
    running.id = "run-2".to_string();
    running.task_id = "task-2".to_string();
    running.status = TaskRunStatus::Running;
    running.cancel_requested = true;
    running.cancel_event_pending = true;
    running.worker_id = Some("worker-1".to_string());
    store.save_run(running).expect("save running run");

    let mut succeeded = queued_run();
    succeeded.id = "run-3".to_string();
    succeeded.task_id = "task-3".to_string();
    succeeded.status = TaskRunStatus::Succeeded;
    succeeded.model_phase_status = crate::models::ModelPhaseStatus::Succeeded;
    succeeded.dispatch_event_pending = false;
    succeeded.post_process_event_pending = true;
    store.save_run(succeeded).expect("save succeeded run");

    let stats = store.run_execution_stats();

    assert_eq!(stats.total, 3);
    assert_eq!(stats.active, 2);
    assert_eq!(stats.queued, 1);
    assert_eq!(stats.running, 1);
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.dispatch_outbox_pending, 1);
    assert_eq!(stats.cancellation_outbox_pending, 1);
    assert_eq!(stats.post_process_outbox_pending, 1);
}

#[test]
fn running_execution_lane_lookup_excludes_the_waiting_run() {
    let store = test_store();
    let mut active = queued_run();
    active.id = "run-active".to_string();
    active.task_id = "task-active".to_string();
    active.status = TaskRunStatus::Running;
    active.execution_lane_key = Some("project:one".to_string());
    store.save_run(active).expect("save active lane owner");

    let mut waiting = queued_run();
    waiting.id = "run-waiting".to_string();
    waiting.task_id = "task-waiting".to_string();
    waiting.execution_lane_key = Some("project:one".to_string());
    store.save_run(waiting).expect("save queued lane waiter");

    let owner = store
        .get_running_run_for_execution_lane("project:one", "run-waiting")
        .expect("find active lane owner");
    assert_eq!(owner.id, "run-active");
    assert!(store
        .get_running_run_for_execution_lane("project:two", "run-waiting")
        .is_none());
}

#[test]
fn integration_order_uses_ready_time_created_time_and_run_id() {
    let store = test_store();
    let mut first = queued_run();
    first.id = "run-a".to_string();
    first.created_at = "2026-08-15T01:00:00Z".to_string();
    first.workspace_execution = Some(
        serde_json::from_value(serde_json::json!({
            "status": "ready",
            "execution_group_id": "group-1",
            "integration_status": "pending",
            "integration_ready_at": "2026-08-15T02:00:00Z"
        }))
        .expect("first integration state"),
    );
    store.save_run(first).expect("save first run");

    let mut second = queued_run();
    second.id = "run-b".to_string();
    second.created_at = "2026-08-15T01:00:01Z".to_string();
    second.workspace_execution = Some(
        serde_json::from_value(serde_json::json!({
            "status": "ready",
            "execution_group_id": "group-1",
            "integration_status": "pending",
            "integration_ready_at": "2026-08-15T02:00:00Z"
        }))
        .expect("second integration state"),
    );
    store.save_run(second).expect("save second run");

    let prior = store
        .get_prior_pending_integration_run(
            "group-1",
            "2026-08-15T02:00:00Z",
            "2026-08-15T01:00:01Z",
            "run-b",
        )
        .expect("prior run");
    assert_eq!(prior.id, "run-a");
}

#[test]
fn integration_conflict_retry_rearms_the_same_run_without_changing_its_order() {
    let store = test_store();
    let mut run = queued_run();
    run.status = TaskRunStatus::Blocked;
    run.model_phase_status = crate::models::ModelPhaseStatus::Blocked;
    run.finished_at = Some("2026-08-15T03:00:00Z".to_string());
    run.error_message = Some("integration conflict".to_string());
    run.workspace_execution = Some(
        serde_json::from_value(serde_json::json!({
            "status": "ready",
            "execution_group_id": "group-1",
            "integration_status": "conflict",
            "integration_ready_at": "2026-08-15T02:00:00Z",
            "integration_attempt_count": 1,
            "conflict_files": ["src/main.rs"],
            "conflict_message": "same line changed"
        }))
        .expect("conflict integration state"),
    );
    store.save_run(run).expect("save conflict run");

    let retried = store
        .rearm_run_workspace_integration("run-1")
        .expect("rearm integration conflict");
    let integration = retried
        .workspace_execution
        .as_ref()
        .expect("workspace execution");

    assert_eq!(retried.status, TaskRunStatus::Running);
    assert!(retried.finished_at.is_none());
    assert_eq!(
        integration.integration_status,
        WorkspaceIntegrationStatus::Pending
    );
    assert_eq!(
        integration.integration_ready_at.as_deref(),
        Some("2026-08-15T02:00:00Z")
    );
    assert_eq!(integration.integration_attempt_count, 1);
    assert!(integration.conflict_files.is_empty());
    assert!(retried.post_process_event_pending);
}

#[test]
fn integration_conflict_waiver_preserves_result_and_rearms_post_process() {
    let store = test_store();
    let mut run = queued_run();
    run.status = TaskRunStatus::Blocked;
    run.model_phase_status = crate::models::ModelPhaseStatus::Blocked;
    run.finished_at = Some("2026-08-15T03:00:00Z".to_string());
    run.error_message = Some("integration conflict".to_string());
    run.result_summary = Some("optional analysis completed".to_string());
    run.workspace_execution = Some(
        serde_json::from_value(serde_json::json!({
            "status": "ready",
            "execution_group_id": "group-1",
            "integration_status": "conflict",
            "result_commit": "result-commit-1",
            "integration_attempt_count": 1,
            "conflict_files": ["src/main.rs"]
        }))
        .expect("conflict integration state"),
    );
    store.save_run(run).expect("save conflict run");

    let waived = store
        .waive_run_workspace_integration("run-1", "optional change is not needed")
        .expect("waive integration conflict");
    let integration = waived
        .workspace_execution
        .as_ref()
        .expect("workspace execution");

    assert_eq!(waived.status, TaskRunStatus::Succeeded);
    assert!(waived.finished_at.is_some());
    assert_eq!(
        waived.result_summary.as_deref(),
        Some("optional analysis completed")
    );
    assert_eq!(
        integration.integration_status,
        WorkspaceIntegrationStatus::Waived
    );
    assert_eq!(
        integration.result_commit.as_deref(),
        Some("result-commit-1")
    );
    assert_eq!(
        integration.waiver_reason.as_deref(),
        Some("optional change is not needed")
    );
    assert!(integration.waived_at.is_some());
    assert!(waived.post_process_event_pending);
    assert!(!waived.post_process_completed);
}

#[test]
fn stale_cancel_repair_updates_terminal_runs_in_place() {
    let store = test_store();
    let mut terminal = queued_run();
    terminal.status = TaskRunStatus::Succeeded;
    store.save_run(terminal).expect("save terminal run");
    store
        .mark_cancel_requested("run-1")
        .expect("mark terminal cancellation");

    let mut running = queued_run();
    running.id = "run-2".to_string();
    running.task_id = "task-2".to_string();
    running.status = TaskRunStatus::Running;
    running.worker_id = Some("worker-1".to_string());
    store.save_run(running).expect("save running run");
    store
        .mark_cancel_requested("run-2")
        .expect("mark running cancellation");

    assert_eq!(store.repair_stale_cancel_requested_runs(), 1);
    assert!(
        !store
            .get_run("run-1")
            .expect("terminal run")
            .cancel_requested
    );
    assert!(
        store
            .get_run("run-2")
            .expect("running run")
            .cancel_requested
    );
}

fn run_event(id: &str, created_at: &str) -> TaskRunEventRecord {
    TaskRunEventRecord {
        id: id.to_string(),
        run_id: "run-1".to_string(),
        event_type: "task.log".to_string(),
        message: Some(id.to_string()),
        payload: None,
        created_at: created_at.to_string(),
    }
}

#[test]
fn successful_run_post_process_outbox_is_monotonic_across_stale_saves() {
    let store = test_store();
    let mut run = queued_run();
    run.status = TaskRunStatus::Succeeded;
    run.model_phase_status = crate::models::ModelPhaseStatus::Succeeded;
    run.finished_at = Some(now_rfc3339());
    let saved = store.save_run(run).expect("save successful run");
    assert!(saved.post_process_event_pending);
    assert!(!saved.post_process_event_enqueued);

    let mut stale = saved.clone();
    assert!(store.acknowledge_run_post_process_event(saved.id.as_str()));
    stale.result_summary = Some("late callback save".to_string());
    let merged = store.save_run(stale).expect("merge stale terminal save");
    assert!(!merged.post_process_event_pending);
    assert!(merged.post_process_event_enqueued);

    assert!(store.mark_run_memory_summary_processed(saved.id.as_str(), Some("job-1")));
    assert!(store.mark_run_chatos_followup_processed(saved.id.as_str()));
    assert!(store.mark_run_post_process_completed(saved.id.as_str()));
    let merged = store.save_run(saved).expect("preserve completed progress");
    assert!(merged.post_process_completed);
    assert!(merged.memory_summary_processed);
    assert!(merged.chatos_followup_processed);
    assert_eq!(merged.summary_job_run_id.as_deref(), Some("job-1"));
    assert!(store.list_pending_run_post_processes(10).is_empty());
}

#[test]
fn every_terminal_status_requests_run_lifecycle_post_process() {
    for (index, status) in [
        TaskRunStatus::Succeeded,
        TaskRunStatus::Failed,
        TaskRunStatus::Cancelled,
        TaskRunStatus::Blocked,
    ]
    .into_iter()
    .enumerate()
    {
        let store = test_store();
        let mut run = queued_run();
        run.id = format!("run-{index}");
        run.status = status;
        run.model_phase_status = match status {
            TaskRunStatus::Succeeded => crate::models::ModelPhaseStatus::Succeeded,
            TaskRunStatus::Failed => crate::models::ModelPhaseStatus::Failed,
            TaskRunStatus::Cancelled => crate::models::ModelPhaseStatus::Cancelled,
            TaskRunStatus::Blocked => crate::models::ModelPhaseStatus::Blocked,
            TaskRunStatus::Queued | TaskRunStatus::Running => unreachable!(),
        };
        run.finished_at = Some(now_rfc3339());
        let saved = store.save_run(run).expect("save terminal run");
        assert!(saved.post_process_event_pending);
        assert_eq!(store.list_pending_run_post_processes(10).len(), 1);
    }
}

#[test]
fn workspace_integration_waits_for_the_durable_model_terminal_state() {
    let store = test_store();
    let mut run = queued_run();
    run.status = TaskRunStatus::Running;
    run.model_phase_status = crate::models::ModelPhaseStatus::Running;
    run.workspace_execution = Some(
        serde_json::from_value(serde_json::json!({
            "status": "ready",
            "execution_group_id": "group-1",
            "integration_status": "pending"
        }))
        .expect("workspace integration state"),
    );

    let running = store.save_run(run).expect("save running model phase");
    assert!(!running.post_process_event_pending);
    assert!(store.list_pending_run_post_processes(10).is_empty());

    let mut terminal_model = running;
    terminal_model.model_phase_status = crate::models::ModelPhaseStatus::Succeeded;
    let terminal_model = store
        .save_run(terminal_model)
        .expect("save durable model terminal state");
    assert!(terminal_model.post_process_event_pending);
    assert_eq!(store.list_pending_run_post_processes(10).len(), 1);
}

#[test]
fn dead_lettered_post_process_is_not_rearmed_by_later_run_saves() {
    let store = test_store();
    let mut run = queued_run();
    run.status = TaskRunStatus::Succeeded;
    run.model_phase_status = crate::models::ModelPhaseStatus::Succeeded;
    let saved = store.save_run(run).expect("save successful run");
    assert!(store.acknowledge_run_post_process_event(saved.id.as_str()));
    assert!(store.record_run_post_process_failure(saved.id.as_str(), "poison event"));
    assert!(store.mark_run_post_process_dead_lettered(saved.id.as_str(), "poison event"));

    let merged = store.save_run(saved).expect("merge stale terminal save");
    assert!(merged.post_process_dead_lettered);
    assert!(!merged.post_process_event_pending);
    assert!(!merged.post_process_event_enqueued);
    assert!(store.list_pending_run_post_processes(10).is_empty());
}

#[test]
fn dead_lettered_post_process_can_only_be_explicitly_rearmed() {
    let store = test_store();
    let mut run = queued_run();
    run.status = TaskRunStatus::Succeeded;
    run.model_phase_status = crate::models::ModelPhaseStatus::Succeeded;
    run.post_process_dead_lettered = true;
    run.post_process_attempt_count = 8;
    run.post_process_last_error = Some("poison event".to_string());
    let run_id = run.id.clone();
    store.save_run(run).expect("save dead-lettered run");

    assert!(store.rearm_run_post_process_dead_letter(run_id.as_str()));
    let replay = store.get_run(run_id.as_str()).expect("rearmed run");
    assert!(!replay.post_process_dead_lettered);
    assert_eq!(replay.post_process_attempt_count, 0);
    assert!(replay.post_process_event_pending);
    assert!(!replay.post_process_event_enqueued);
    assert!(replay.post_process_last_error.is_none());
    assert!(!store.rearm_run_post_process_dead_letter(run_id.as_str()));
}

#[test]
fn terminal_run_subscription_becomes_publishable_only_after_terminal_state() {
    let store = test_store();
    let run = store.save_run(queued_run()).expect("save queued run");
    let subscription =
        RunTerminalSubscriptionRecord::new(run.id.as_str(), "parent-run-1", "worker-1");
    store
        .subscribe_run_terminal(subscription.clone())
        .expect("subscribe terminal event");
    assert!(store.list_pending_run_terminal_subscriptions(10).is_empty());

    let mut completed = run;
    completed.status = TaskRunStatus::Succeeded;
    store.save_run(completed).expect("save terminal run");

    let pending = store.list_pending_run_terminal_subscriptions(10);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, subscription);
    assert!(store.acknowledge_run_terminal_subscription(subscription.id.as_str()));
    assert!(store.list_pending_run_terminal_subscriptions(10).is_empty());
}

#[test]
fn list_run_events_after_returns_incremental_suffix() {
    let store = test_store();
    store.append_run_event(run_event("evt-1", "2026-08-03T10:00:00Z"));
    store.append_run_event(run_event("evt-2", "2026-08-03T10:00:00Z"));
    store.append_run_event(run_event("evt-3", "2026-08-03T10:00:01Z"));

    let events =
        store.list_run_events_after("run-1", Some("2026-08-03T10:00:00Z"), Some("evt-1"), 10);

    assert_eq!(
        events
            .iter()
            .map(|event| event.id.as_str())
            .collect::<Vec<_>>(),
        vec!["evt-2", "evt-3"]
    );
}

#[test]
fn list_run_events_after_respects_limit() {
    let store = test_store();
    store.append_run_event(run_event("evt-1", "2026-08-03T10:00:00Z"));
    store.append_run_event(run_event("evt-2", "2026-08-03T10:00:01Z"));

    let events = store.list_run_events_after("run-1", None, None, 1);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, "evt-1");
}

#[test]
fn run_event_retention_only_prunes_expired_events_for_terminal_runs() {
    let store = test_store();
    let mut terminal = queued_run();
    terminal.status = TaskRunStatus::Succeeded;
    terminal.finished_at = Some("2026-07-01T00:00:00Z".to_string());
    store.save_run(terminal).expect("save terminal run");
    store.append_run_event(run_event("old-terminal", "2026-07-01T00:00:00Z"));
    store.append_run_event(run_event("new-terminal", "2026-08-02T00:00:00Z"));

    let mut active = queued_run();
    active.id = "run-2".to_string();
    active.task_id = "task-2".to_string();
    active.status = TaskRunStatus::Running;
    store.save_run(active).expect("save active run");
    let mut active_event = run_event("old-active", "2026-07-01T00:00:00Z");
    active_event.run_id = "run-2".to_string();
    store.append_run_event(active_event);

    let result = store.prune_terminal_run_events_before("2026-08-01T00:00:00Z", 100);

    assert_eq!(result.eligible_runs, 1);
    assert_eq!(result.deleted_events, 1);
    assert_eq!(
        store
            .list_run_events("run-1")
            .into_iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        vec!["new-terminal".to_string()]
    );
    assert_eq!(store.list_run_events("run-2").len(), 1);
}

#[test]
fn latest_run_event_cursor_uses_persisted_sort_order() {
    let store = test_store();
    store.append_run_event(run_event("evt-2", "2026-08-03T10:00:00Z"));
    store.append_run_event(run_event("evt-1", "2026-08-03T10:00:01Z"));

    assert_eq!(
        store.latest_run_event_cursor("run-1"),
        Some(("2026-08-03T10:00:01Z".to_string(), "evt-1".to_string()))
    );
}
