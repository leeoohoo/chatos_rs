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
        execution_lane_key: None,
        model_config_id: "model-1".to_string(),
        memory_thread_id: "thread-1".to_string(),
        status: TaskRunStatus::Queued,
        started_at: None,
        finished_at: None,
        input_snapshot: serde_json::json!({}),
        plugin_snapshots: Vec::new(),
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
        terminal_cleanup_event_pending: false,
        terminal_cleanup_event_enqueued: false,
        terminal_cleanup_completed: false,
        terminal_cleanup_attempt_count: 0,
        terminal_cleanup_last_error: None,
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
    succeeded.dispatch_event_pending = false;
    succeeded.post_process_event_pending = true;
    succeeded.terminal_cleanup_event_pending = true;
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
    assert_eq!(stats.terminal_cleanup_outbox_pending, 1);
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

#[test]
fn execution_lane_allows_only_one_running_project_task() {
    let store = test_store();
    let mut first = queued_run();
    first.execution_lane_key = Some("project:one".to_string());
    store.save_run(first).expect("save first run");

    let mut second = queued_run();
    second.id = "run-2".to_string();
    second.task_id = "task-2".to_string();
    second.execution_lane_key = Some("project:one".to_string());
    store.save_run(second).expect("save second run");

    let mut other_project = queued_run();
    other_project.id = "run-3".to_string();
    other_project.task_id = "task-3".to_string();
    other_project.execution_lane_key = Some("project:two".to_string());
    store
        .save_run(other_project)
        .expect("save other project run");

    let claimed_first = store
        .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
        .expect("claim first lane");
    assert_eq!(claimed_first.id, "run-1");

    let claimed_other = store
        .claim_next_queued_run("worker-2", "claim-3", "2999-01-01T00:00:00Z")
        .expect("claim other project");
    assert_eq!(claimed_other.id, "run-3");

    let mut finished_first = claimed_first;
    finished_first.status = TaskRunStatus::Succeeded;
    finished_first.updated_at = now_rfc3339();
    store.save_run(finished_first).expect("finish first lane");

    let claimed_second = store
        .claim_next_queued_run("worker-3", "claim-4", "2999-01-01T00:00:00Z")
        .expect("claim released lane");
    assert_eq!(claimed_second.id, "run-2");
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
fn terminal_save_with_matching_claim_clears_claim_metadata() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");
    let mut claimed = store
        .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
        .expect("claim run");

    claimed.status = TaskRunStatus::Succeeded;
    claimed.finished_at = Some(now_rfc3339());
    claimed.updated_at = now_rfc3339();
    let saved = store.save_run(claimed).expect("save terminal run");

    assert_eq!(saved.status, TaskRunStatus::Succeeded);
    assert_eq!(saved.attempts.len(), 1);
    assert_eq!(saved.attempts[0].status, TaskRunAttemptStatus::Succeeded);
    assert!(saved.attempts[0].finished_at.is_some());
    assert_eq!(saved.worker_id.as_deref(), Some("worker-1"));
    assert!(saved.claim_token.is_none());
    assert!(saved.claim_until.is_none());
    assert_eq!(
        saved
            .chatos_callback_delivery
            .as_ref()
            .map(|state| state.status),
        Some(ChatosCallbackDeliveryStatus::Pending)
    );
    let persisted = store.get_run("run-1").expect("persisted run");
    assert!(persisted.claim_token.is_none());
    assert!(persisted.claim_until.is_none());
}

#[test]
fn paused_queued_run_is_not_claimed_until_resumed() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");
    assert_eq!(
        store.set_queued_runs_dispatch_paused(&["task-1".to_string()], true),
        1
    );
    assert!(store
        .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
        .is_none());

    assert_eq!(
        store.set_queued_runs_dispatch_paused(&["task-1".to_string()], false),
        1
    );
    assert!(store
        .claim_next_queued_run("worker-1", "claim-2", "2999-01-01T00:00:00Z")
        .is_some());
}

#[test]
fn paused_queued_run_is_not_waiting_or_claimable() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");
    assert!(store.has_queued_run_waiting_for_execution());
    assert_eq!(
        store.set_queued_runs_dispatch_paused(&["task-1".to_string()], true),
        1
    );

    assert!(!store.has_queued_run_waiting_for_execution());
    assert!(store
        .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
        .is_none());
}

#[test]
fn queued_run_dispatch_outbox_is_acknowledgeable() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");

    let pending = store.list_pending_run_dispatches(10);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "run-1");
    assert!(store.acknowledge_run_dispatch_event("run-1"));
    assert!(store.list_pending_run_dispatches(10).is_empty());
    assert!(!store.acknowledge_run_dispatch_event("run-1"));
}

#[test]
fn successful_run_post_process_outbox_is_monotonic_across_stale_saves() {
    let store = test_store();
    let mut run = queued_run();
    run.status = TaskRunStatus::Succeeded;
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
fn terminal_cleanup_failure_returns_event_to_outbox_until_completed() {
    let store = test_store();
    let mut run = queued_run();
    run.status = TaskRunStatus::Failed;
    run.worker_id = Some("worker-1".to_string());
    run.terminal_cleanup_event_pending = true;
    store.save_run(run).expect("save terminal cleanup request");

    assert_eq!(store.list_pending_terminal_cleanups(10).len(), 1);
    assert!(store.acknowledge_terminal_cleanup_event("run-1"));
    assert!(store.list_pending_terminal_cleanups(10).is_empty());
    assert!(store.retry_terminal_cleanup("run-1", "temporary failure"));
    assert_eq!(store.list_pending_terminal_cleanups(10).len(), 1);
    assert!(store.mark_terminal_cleanup_completed("run-1"));
    assert!(store.list_pending_terminal_cleanups(10).is_empty());
    let run = store.get_run("run-1").expect("stored run");
    assert!(run.terminal_cleanup_completed);
    assert_eq!(run.terminal_cleanup_attempt_count, 1);
    assert!(run.terminal_cleanup_last_error.is_none());
}

#[test]
fn dead_lettered_post_process_is_not_rearmed_by_later_run_saves() {
    let store = test_store();
    let mut run = queued_run();
    run.status = TaskRunStatus::Succeeded;
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
fn running_run_cancel_outbox_is_acknowledgeable() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");
    store
        .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
        .expect("claim run");

    let cancelled = store
        .mark_cancel_requested("run-1")
        .expect("mark cancel requested");
    assert!(cancelled.cancel_event_pending);
    assert_eq!(store.list_pending_run_cancel_events(10).len(), 1);
    assert!(store.acknowledge_run_cancel_event("run-1"));
    assert!(store.list_pending_run_cancel_events(10).is_empty());
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
fn stale_worker_cannot_save_after_claim_expires() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");
    let mut stale = store
        .claim_next_queued_run("worker-1", "claim-1", "2000-01-01T00:00:00Z")
        .expect("claim run");
    stale.attempt = 3;
    stale = store.save_run(stale).expect("persist exhausted attempts");

    let failed_runs =
        store.reconcile_expired_run_claims("2001-01-01T00:00:00Z", "2001-01-01T00:01:00Z", 3);
    assert_eq!(failed_runs.len(), 1);
    assert_eq!(failed_runs[0].id, "run-1");
    assert_eq!(
        failed_runs[0].finished_at.as_deref(),
        Some("2001-01-01T00:01:00Z")
    );
    stale.status = TaskRunStatus::Succeeded;
    stale.finished_at = Some(now_rfc3339());
    stale.updated_at = now_rfc3339();

    let err = store.save_run(stale).expect_err("stale claim rejected");
    assert!(err.contains("run claim lost"));
    let persisted = store.get_run("run-1").expect("persisted run");
    assert_eq!(persisted.status, TaskRunStatus::Failed);
    assert_eq!(
        persisted.error_message.as_deref(),
        Some("worker claim expired")
    );
}

#[test]
fn expired_claim_is_requeued_before_attempt_limit() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");
    let first_claim = store
        .claim_next_queued_run("worker-1", "claim-1", "2000-01-01T00:00:00Z")
        .expect("claim run");
    let original_started_at = first_claim.started_at.clone().expect("first start time");
    assert_eq!(first_claim.attempts.len(), 1);
    assert_eq!(first_claim.attempts[0].attempt_id, "claim-1");
    assert_eq!(
        first_claim.attempts[0].status,
        TaskRunAttemptStatus::Running
    );

    let reconciled =
        store.reconcile_expired_run_claims("2001-01-01T00:00:00Z", "2001-01-01T00:01:00Z", 3);

    assert_eq!(reconciled.len(), 1);
    assert_eq!(reconciled[0].status, TaskRunStatus::Queued);
    assert_eq!(reconciled[0].attempt, 1);
    assert_eq!(reconciled[0].attempts.len(), 1);
    assert_eq!(
        reconciled[0].attempts[0].status,
        TaskRunAttemptStatus::Interrupted
    );
    assert_eq!(
        reconciled[0].attempts[0].finished_at.as_deref(),
        Some("2001-01-01T00:01:00Z")
    );
    assert_eq!(
        reconciled[0].started_at.as_deref(),
        Some(original_started_at.as_str())
    );
    assert!(reconciled[0].finished_at.is_none());
    assert!(reconciled[0].claim_token.is_none());
    assert!(reconciled[0].claim_until.is_none());
    assert!(reconciled[0].chatos_callback_delivery.is_none());
    let reclaimed = store
        .claim_next_queued_run("worker-2", "claim-2", "2002-01-01T00:00:00Z")
        .expect("reclaim recovered run");
    assert_eq!(reclaimed.attempt, 2);
    assert_eq!(reclaimed.attempts.len(), 2);
    assert_eq!(reclaimed.attempts[1].attempt_id, "claim-2");
    assert_eq!(reclaimed.attempts[1].sequence, 2);
    assert_eq!(
        reclaimed.attempts[1].recovery_reason.as_deref(),
        Some("worker_claim_expired")
    );
    assert_eq!(reclaimed.attempts[1].status, TaskRunAttemptStatus::Running);
    assert_eq!(
        reclaimed.started_at.as_deref(),
        Some(original_started_at.as_str())
    );
    assert!(reclaimed.result_summary.is_none());
}

#[test]
fn expired_cancel_requested_claim_becomes_cancelled() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");
    store
        .claim_next_queued_run("worker-1", "claim-1", "2000-01-01T00:00:00Z")
        .expect("claim run");
    store
        .mark_cancel_requested("run-1")
        .expect("mark cancel requested");

    let terminal_runs =
        store.reconcile_expired_run_claims("2001-01-01T00:00:00Z", "2001-01-01T00:01:00Z", 3);

    assert_eq!(terminal_runs.len(), 1);
    assert_eq!(terminal_runs[0].status, TaskRunStatus::Cancelled);
    assert_eq!(
        terminal_runs[0].result_summary.as_deref(),
        Some("任务取消请求已生效；运行节点心跳过期后按取消收尾")
    );
    assert_eq!(terminal_runs[0].error_message, None);
    assert_eq!(
        terminal_runs[0]
            .chatos_callback_delivery
            .as_ref()
            .map(|delivery| delivery.event.as_str()),
        Some("task.cancelled")
    );
    assert!(!store.is_cancel_requested("run-1"));
}

#[test]
fn claim_is_not_failed_before_expiry_cutoff() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");
    store
        .claim_next_queued_run("worker-1", "claim-1", "2001-01-01T00:00:00Z")
        .expect("claim run");

    assert!(store
        .reconcile_expired_run_claims("2000-12-31T23:59:59Z", "2001-01-01T00:01:00Z", 3)
        .is_empty());
    assert_eq!(
        store.get_run("run-1").expect("persisted run").status,
        TaskRunStatus::Running
    );
}

#[test]
fn local_abort_signal_does_not_mutate_persisted_cancel_flag() {
    let store = test_store();
    store.save_run(queued_run()).expect("save queued run");

    store.signal_local_run_abort("run-1");
    assert!(store.is_cancel_requested("run-1"));
    assert!(!store.get_run("run-1").expect("run").cancel_requested);

    store.clear_local_run_abort("run-1");
    assert!(!store.is_cancel_requested("run-1"));
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
