// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::lifecycle::{sanitize_terminal_result, MAX_INLINE_MCP_RESULT_BYTES};
use super::*;
use chatos_mcp_service::MCP_ERROR_INTERNAL;

fn record() -> RuntimeInvocationRecord {
    RuntimeInvocationRecord {
        invocation_id: "invocation-1".to_string(),
        session_id: "session-1".to_string(),
        request_id_key: "\"request-1\"".to_string(),
        caller_service: "task-runner".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        project_id: Some("project-1".to_string()),
        device_id: None,
        resource_id: "mcp-1".to_string(),
        exposed_tool_name: "demo_read".to_string(),
        original_tool_name: "demo_read".to_string(),
        mutation_may_have_started: false,
        cancel_supported: true,
        status: RuntimeInvocationStatus::Running,
        created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
        started_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
        completed_at_unix_ms: None,
        terminal_result: None,
        terminal_error_code: None,
        terminal_error_message: None,
        file_modification_outcome: None,
        expires_at: DateTime::from_millis((chrono::Utc::now().timestamp() + 60) * 1_000),
        expires_at_unix: chrono::Utc::now().timestamp() + 60,
    }
}

#[test]
fn terminal_result_keeps_supported_visual_payloads_inline() {
    let result = serde_json::json!({
        "content": [{
            "type": "image",
            "mimeType": "image/png",
            "data": "a".repeat(MAX_INLINE_MCP_RESULT_BYTES - 256),
        }]
    });

    assert_eq!(
        sanitize_terminal_result(Some(result.clone())).unwrap(),
        Some(result)
    );
}

#[test]
fn terminal_result_truncates_only_after_visual_payload_budget_is_exceeded() {
    let result = serde_json::json!({
        "content": [{
            "type": "image",
            "mimeType": "image/png",
            "data": "a".repeat(MAX_INLINE_MCP_RESULT_BYTES),
        }]
    });
    let encoded_bytes = serde_json::to_vec(&result).unwrap().len();

    assert_eq!(
        sanitize_terminal_result(Some(result)).unwrap(),
        Some(serde_json::json!({
            "status": "result_truncated",
            "result_bytes": encoded_bytes,
        }))
    );
}

#[tokio::test]
async fn cloned_store_coordinates_cancel_and_terminal_transition() {
    let writer = RuntimeInvocationStore::memory();
    let reader = writer.clone();
    writer.register(record()).await.unwrap();
    assert!(writer.mark_waiting_for_user("invocation-1").await.unwrap());
    let cancelled = reader
        .request_cancel_by_request("session-1", "\"request-1\"")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.status, RuntimeInvocationStatus::CancelRequested);
    assert!(writer.cancellation_requested("invocation-1").await.unwrap());
    assert!(reader
        .finish_cancellation("invocation-1", RuntimeInvocationStatus::Cancelled)
        .await
        .unwrap());
    assert!(!writer.cancellation_requested("invocation-1").await.unwrap());
}

#[tokio::test]
async fn cancellation_waiter_is_released_by_event_signal_without_polling() {
    let store = RuntimeInvocationStore::memory();
    store.register(record()).await.unwrap();
    let waiter_store = store.clone();
    let waiter =
        tokio::spawn(async move { waiter_store.wait_for_cancellation("invocation-1").await });
    tokio::task::yield_now().await;

    store
        .request_cancel_by_invocation("invocation-1", "task-runner")
        .await
        .unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
        .await
        .expect("cancellation waiter should be event-driven")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn completed_invocation_cannot_be_changed_to_cancel_requested() {
    let store = RuntimeInvocationStore::memory();
    store.register(record()).await.unwrap();
    assert!(store
        .complete("invocation-1", serde_json::json!({"ok": true}))
        .await
        .unwrap());
    let completed = store
        .request_cancel_by_invocation("invocation-1", "task-runner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, RuntimeInvocationStatus::Completed);
}

#[tokio::test]
async fn terminal_process_wait_timeout_is_recorded_as_failed_with_result_preserved() {
    let store = RuntimeInvocationStore::memory();
    let mut invocation = record();
    invocation.exposed_tool_name = "sandbox_process_wait".to_string();
    invocation.original_tool_name = "process_wait".to_string();
    store.register(invocation).await.unwrap();

    let result = serde_json::json!({
        "_structured_result": {
            "terminal_id": "terminal-1",
            "wait_status": "timeout",
            "completed": false,
            "timed_out": true,
            "waited_ms": 7_200_000
        }
    });
    assert!(store
        .complete("invocation-1", result.clone())
        .await
        .unwrap());

    let failed = store
        .get_for_caller("invocation-1", "task-runner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, RuntimeInvocationStatus::Failed);
    assert_eq!(failed.terminal_result, Some(result));
    assert_eq!(
        failed.terminal_error_code,
        Some(chatos_mcp_service::MCP_ERROR_INTERNAL)
    );
    assert_eq!(
        failed.terminal_error_message.as_deref(),
        Some("terminal process wait timed out")
    );
}

#[tokio::test]
async fn unrelated_timed_out_result_is_not_reclassified_as_process_wait_failure() {
    let store = RuntimeInvocationStore::memory();
    store.register(record()).await.unwrap();

    assert!(store
        .complete(
            "invocation-1",
            serde_json::json!({"timed_out": true, "completed": false})
        )
        .await
        .unwrap());

    let completed = store
        .get_for_caller("invocation-1", "task-runner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, RuntimeInvocationStatus::Completed);
    assert_eq!(completed.terminal_error_code, None);
}

#[tokio::test]
async fn completed_request_id_can_be_reused_after_the_prior_call_is_terminal() {
    let store = RuntimeInvocationStore::memory();
    store.register(record()).await.unwrap();
    assert!(store
        .complete("invocation-1", serde_json::json!({"ok": true}))
        .await
        .unwrap());
    let mut reused = record();
    reused.invocation_id = "invocation-2".to_string();
    store.register(reused).await.unwrap();
    assert!(store
        .request_cancel_by_invocation("invocation-2", "task-runner")
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn duplicate_active_request_id_has_a_distinct_registration_error() {
    let store = RuntimeInvocationStore::memory();
    store.register(record()).await.unwrap();
    let mut duplicate = record();
    duplicate.invocation_id = "invocation-duplicate".to_string();

    assert_eq!(
        store.register(duplicate).await.unwrap_err(),
        RuntimeInvocationRegisterError::DuplicateActiveId
    );
    let stats = store.stats().await.unwrap();
    assert_eq!(stats.registration.duplicate_active_id, 1);
    assert_eq!(stats.total_active, 1);
}

#[tokio::test]
async fn ten_thousand_sequential_calls_reuse_request_id_without_quota_leaks() {
    let store = RuntimeInvocationStore::memory();
    for index in 0..10_000 {
        let mut invocation = record();
        invocation.invocation_id = format!("invocation-sequential-{index}");
        store.register(invocation).await.unwrap();
        assert!(store
            .complete(
                format!("invocation-sequential-{index}").as_str(),
                serde_json::json!({"ok": true}),
            )
            .await
            .unwrap());
    }

    let stats = store.stats().await.unwrap();
    assert_eq!(stats.total_active, 0);
    assert_eq!(stats.registration.duplicate_active_id, 0);
    assert_eq!(stats.quota_release_failures_total, 0);
    assert_eq!(stats.duration.completed_count, 1);
}

#[tokio::test]
async fn closing_session_terminalizes_active_calls_and_releases_quota() {
    let store = RuntimeInvocationStore::memory_with_quota(
        RuntimeInvocationQuotaLimits::new(2, 2, 2, 2).unwrap(),
    );
    let running = record();
    store.register(running.clone()).await.unwrap();
    let mut queued_mutation = record();
    queued_mutation.invocation_id = "invocation-queued-mutation".to_string();
    queued_mutation.request_id_key = "\"request-queued-mutation\"".to_string();
    queued_mutation.status = RuntimeInvocationStatus::Queued;
    queued_mutation.started_at_unix_ms = None;
    queued_mutation.mutation_may_have_started = true;
    store.register(queued_mutation.clone()).await.unwrap();

    assert_eq!(store.close_session("session-1").await.unwrap(), 2);
    assert_eq!(
        store
            .get_for_caller(running.invocation_id.as_str(), "task-runner")
            .await
            .unwrap()
            .unwrap()
            .status,
        RuntimeInvocationStatus::Cancelled
    );
    assert_eq!(
        store
            .get_for_caller(queued_mutation.invocation_id.as_str(), "task-runner")
            .await
            .unwrap()
            .unwrap()
            .status,
        RuntimeInvocationStatus::Cancelled
    );
    let stats = store.stats().await.unwrap();
    assert_eq!(stats.total_active, 0);
    assert_eq!(stats.session_closed_reclaimed_total, 2);

    let mut next = record();
    next.invocation_id = "invocation-after-session-close".to_string();
    next.session_id = "session-2".to_string();
    next.request_id_key = "\"request-after-session-close\"".to_string();
    store.register(next).await.unwrap();
}

#[tokio::test]
async fn closing_session_marks_started_mutation_as_unknown_execution_state() {
    let store = RuntimeInvocationStore::memory();
    let mut mutation = record();
    mutation.mutation_may_have_started = true;
    store.register(mutation.clone()).await.unwrap();

    assert_eq!(store.close_session("session-1").await.unwrap(), 1);
    let closed = store
        .get_for_caller(mutation.invocation_id.as_str(), "task-runner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        closed.status,
        RuntimeInvocationStatus::UnknownExecutionState
    );
    assert_eq!(
        closed.terminal_error_message.as_deref(),
        Some("runtime_session_closed")
    );
}

#[tokio::test]
async fn atomic_quota_rejects_excess_calls_and_terminal_state_releases_capacity() {
    let store = RuntimeInvocationStore::memory_with_quota(
        RuntimeInvocationQuotaLimits::new(1, 1, 1, 1).unwrap(),
    );
    let first = record();
    store.register(first.clone()).await.unwrap();

    let mut second = record();
    second.invocation_id = "invocation-2".to_string();
    second.session_id = "session-2".to_string();
    second.request_id_key = "\"request-2\"".to_string();
    assert_eq!(
        store.register(second.clone()).await.unwrap_err(),
        RuntimeInvocationRegisterError::CapacityExhausted {
            dimension: "tenant",
            limit: 1,
        }
    );

    assert!(store
        .complete(
            first.invocation_id.as_str(),
            serde_json::json!({"ok": true})
        )
        .await
        .unwrap());
    store.register(second).await.unwrap();
}

#[tokio::test]
async fn waiting_for_user_keeps_quota_reserved_until_completion() {
    let store = RuntimeInvocationStore::memory_with_quota(
        RuntimeInvocationQuotaLimits::new(1, 1, 1, 1).unwrap(),
    );
    let first = record();
    store.register(first.clone()).await.unwrap();
    assert!(store
        .mark_waiting_for_user(first.invocation_id.as_str())
        .await
        .unwrap());

    let mut second = record();
    second.invocation_id = "invocation-waiting-quota-2".to_string();
    second.session_id = "session-waiting-quota-2".to_string();
    second.request_id_key = "\"request-waiting-quota-2\"".to_string();
    assert!(matches!(
        store.register(second.clone()).await,
        Err(RuntimeInvocationRegisterError::CapacityExhausted { .. })
    ));

    assert!(store
        .complete(
            first.invocation_id.as_str(),
            serde_json::json!({"answer": "yes"})
        )
        .await
        .unwrap());
    store.register(second).await.unwrap();
}

#[tokio::test]
#[ignore = "requires CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL"]
async fn mongodb_store_coordinates_cancellation_across_service_instances() {
    let database_url = std::env::var("CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL")
        .expect("CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL");
    let invocation_id = format!("shared-invocation-test-{}", uuid::Uuid::new_v4());
    let mut invocation = record();
    invocation.invocation_id = invocation_id.clone();
    invocation.session_id = format!("shared-session-test-{}", uuid::Uuid::new_v4());
    let quota = RuntimeInvocationQuota::memory(
        RuntimeInvocationQuotaLimits::new(100, 100, 100, 100).unwrap(),
    );
    let first = RuntimeInvocationStore::connect(database_url.as_str(), quota.clone())
        .await
        .unwrap();
    let second = RuntimeInvocationStore::connect(database_url.as_str(), quota)
        .await
        .unwrap();
    first.register(invocation.clone()).await.unwrap();
    let cancelled = second
        .request_cancel_by_request(
            invocation.session_id.as_str(),
            invocation.request_id_key.as_str(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.invocation_id, invocation_id);
    let mut duplicate = invocation.clone();
    duplicate.invocation_id = format!("shared-invocation-duplicate-{}", uuid::Uuid::new_v4());
    assert_eq!(
        second.register(duplicate).await.unwrap_err(),
        RuntimeInvocationRegisterError::DuplicateActiveId
    );
    assert!(first
        .cancellation_requested(cancelled.invocation_id.as_str())
        .await
        .unwrap());
    assert!(first
        .finish_cancellation(
            cancelled.invocation_id.as_str(),
            RuntimeInvocationStatus::Cancelled,
        )
        .await
        .unwrap());
    let mut reused = invocation;
    let reused_id = format!("shared-invocation-reused-{}", uuid::Uuid::new_v4());
    reused.invocation_id = reused_id.clone();
    first.register(reused).await.unwrap();
    assert!(first
        .complete(reused_id.as_str(), serde_json::json!({"ok": true}))
        .await
        .unwrap());
    let stats = first.stats().await.unwrap();
    assert_eq!(stats.total_active, 0);
    assert_eq!(stats.registration.duplicate_active_id, 0);
}

#[tokio::test]
async fn queued_invocation_can_be_marked_running_and_failed_with_queryable_error() {
    let store = RuntimeInvocationStore::memory();
    let mut queued = record();
    queued.invocation_id = "invocation-queued".to_string();
    queued.status = RuntimeInvocationStatus::Queued;
    queued.started_at_unix_ms = None;
    store.register(queued).await.unwrap();
    assert!(store.mark_running("invocation-queued").await.unwrap());
    assert!(store
        .fail("invocation-queued", -32000, "provider timed out")
        .await
        .unwrap());
    let record = store
        .get_for_caller("invocation-queued", "task-runner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.status, RuntimeInvocationStatus::Failed);
    assert_eq!(record.terminal_error_code, Some(-32000));
    assert_eq!(
        record.terminal_error_message.as_deref(),
        Some("provider timed out")
    );
}

#[tokio::test]
async fn queued_invocation_can_fail_before_provider_execution_starts() {
    let store = RuntimeInvocationStore::memory();
    let mut queued = record();
    queued.invocation_id = "invocation-queued-dispatch-failure".to_string();
    queued.status = RuntimeInvocationStatus::Queued;
    queued.started_at_unix_ms = None;
    store.register(queued).await.unwrap();

    assert!(store
        .fail(
            "invocation-queued-dispatch-failure",
            -32000,
            "dispatch retries exhausted",
        )
        .await
        .unwrap());
    let record = store
        .get_for_caller("invocation-queued-dispatch-failure", "task-runner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.status, RuntimeInvocationStatus::Failed);
    assert_eq!(
        record.terminal_error_message.as_deref(),
        Some("dispatch retries exhausted")
    );
}

#[tokio::test]
async fn file_modification_outcomes_are_persisted_and_aggregated() {
    let store = RuntimeInvocationStore::memory();

    let mut already_applied = record();
    already_applied.invocation_id = "invocation-edit-already-applied".to_string();
    already_applied.session_id = "session-edit-already-applied".to_string();
    already_applied.request_id_key = "\"request-edit-already-applied\"".to_string();
    already_applied.exposed_tool_name = "harness_code_stage_edit_batch".to_string();
    already_applied.original_tool_name = "stage_edit_batch".to_string();
    store.register(already_applied).await.unwrap();
    assert!(store
        .complete(
            "invocation-edit-already-applied",
            serde_json::json!({
                "_structured_result": {
                    "outcome": "already_applied",
                    "changed": false
                }
            }),
        )
        .await
        .unwrap());

    let mut stale = record();
    stale.invocation_id = "invocation-patch-stale".to_string();
    stale.session_id = "session-patch-stale".to_string();
    stale.request_id_key = "\"request-patch-stale\"".to_string();
    stale.exposed_tool_name = "harness_code_commit_edit_session".to_string();
    stale.original_tool_name = "commit_edit_session".to_string();
    store.register(stale).await.unwrap();
    assert!(store
        .fail(
            "invocation-patch-stale",
            -32000,
            "Patch context not found in file.",
        )
        .await
        .unwrap());

    let completed = store
        .get_for_caller("invocation-edit-already-applied", "task-runner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        completed.file_modification_outcome,
        Some(FileModificationOutcome::AlreadyApplied)
    );
    let failed = store
        .get_for_caller("invocation-patch-stale", "task-runner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        failed.file_modification_outcome,
        Some(FileModificationOutcome::StaleContext)
    );

    let stats = store.stats().await.unwrap();
    assert_eq!(stats.file_modifications.total, 2);
    assert_eq!(stats.file_modifications.already_applied, 1);
    assert_eq!(stats.file_modifications.stale_context, 1);
    assert_eq!(stats.file_modifications.expected_match, 0);
    assert_eq!(stats.file_modifications.changed, 0);
    assert_eq!(stats.file_modifications.validation, 0);
    assert_eq!(stats.file_modifications.infrastructure, 0);
}

#[tokio::test]
async fn non_file_tool_errors_are_not_counted_as_file_modifications() {
    let store = RuntimeInvocationStore::memory();
    store.register(record()).await.unwrap();
    assert!(store
        .fail("invocation-1", -32000, "connection reset")
        .await
        .unwrap());

    let stored = store
        .get_for_caller("invocation-1", "task-runner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.file_modification_outcome, None);
    assert_eq!(store.stats().await.unwrap().file_modifications.total, 0);
}

#[tokio::test]
async fn stats_summarize_memory_store_by_status() {
    let store = RuntimeInvocationStore::memory();

    let queued = RuntimeInvocationRecord {
        invocation_id: "invocation-stats-queued".to_string(),
        session_id: "session-stats-queued".to_string(),
        request_id_key: "\"request-stats-queued\"".to_string(),
        status: RuntimeInvocationStatus::Queued,
        started_at_unix_ms: None,
        ..record()
    };
    let running = RuntimeInvocationRecord {
        invocation_id: "invocation-stats-running".to_string(),
        session_id: "session-stats-running".to_string(),
        request_id_key: "\"request-stats-running\"".to_string(),
        status: RuntimeInvocationStatus::Running,
        ..record()
    };
    let waiting = RuntimeInvocationRecord {
        invocation_id: "invocation-stats-waiting".to_string(),
        session_id: "session-stats-waiting".to_string(),
        request_id_key: "\"request-stats-waiting\"".to_string(),
        status: RuntimeInvocationStatus::Running,
        ..record()
    };
    let terminal = RuntimeInvocationRecord {
        invocation_id: "invocation-stats-terminal".to_string(),
        session_id: "session-stats-terminal".to_string(),
        request_id_key: "\"request-stats-terminal\"".to_string(),
        status: RuntimeInvocationStatus::Completed,
        completed_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
        ..record()
    };

    store.register(queued).await.unwrap();
    store.register(running).await.unwrap();
    store.register(waiting).await.unwrap();
    assert!(store
        .mark_waiting_for_user("invocation-stats-waiting")
        .await
        .unwrap());
    let mut terminal_ready = terminal.clone();
    terminal_ready.status = RuntimeInvocationStatus::Running;
    terminal_ready.completed_at_unix_ms = None;
    store.register(terminal_ready).await.unwrap();
    store
        .complete("invocation-stats-terminal", serde_json::json!({"ok": true}))
        .await
        .unwrap();

    let stats = store.stats().await.unwrap();
    assert_eq!(stats.backend, "memory");
    assert_eq!(stats.total_active, 3);
    assert_eq!(stats.queued, 1);
    assert_eq!(stats.running, 1);
    assert_eq!(stats.waiting_for_user, 1);
    assert_eq!(stats.cancel_requested, 0);
    assert_eq!(stats.terminal, 1);
    assert_eq!(stats.duration.completed_count, 1);
}

#[tokio::test]
async fn restart_recovery_requeues_queued_and_preserves_waiting_user_invocations() {
    let store = RuntimeInvocationStore::memory();
    let mut queued = record();
    queued.invocation_id = "invocation-recovery-queued".to_string();
    queued.request_id_key = "\"request-recovery-queued\"".to_string();
    queued.status = RuntimeInvocationStatus::Queued;
    queued.started_at_unix_ms = None;
    store.register(queued.clone()).await.unwrap();

    let mut waiting = record();
    waiting.invocation_id = "invocation-recovery-waiting".to_string();
    waiting.request_id_key = "\"request-recovery-waiting\"".to_string();
    store.register(waiting.clone()).await.unwrap();
    store
        .mark_waiting_for_user(waiting.invocation_id.as_str())
        .await
        .unwrap();
    waiting.status = RuntimeInvocationStatus::WaitingForUser;

    assert!(!store.recover_after_restart(&queued, true).await.unwrap());
    assert!(!store.recover_after_restart(&waiting, true).await.unwrap());
    let active = store.list_active(10).await.unwrap();
    assert_eq!(active.len(), 2);
}

#[tokio::test]
async fn restart_recovery_fails_read_only_but_marks_started_mutation_unknown() {
    let store = RuntimeInvocationStore::memory();
    let read = record();
    store.register(read.clone()).await.unwrap();
    assert!(store.recover_after_restart(&read, true).await.unwrap());
    let read = store
        .get_for_caller(read.invocation_id.as_str(), read.caller_service.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.status, RuntimeInvocationStatus::Failed);
    assert_eq!(read.terminal_error_code, Some(MCP_ERROR_INTERNAL));

    let mut mutation = record();
    mutation.invocation_id = "invocation-recovery-mutation".to_string();
    mutation.request_id_key = "\"request-recovery-mutation\"".to_string();
    mutation.mutation_may_have_started = true;
    store.register(mutation.clone()).await.unwrap();
    assert!(store.recover_after_restart(&mutation, true).await.unwrap());
    let mutation = store
        .get_for_caller(
            mutation.invocation_id.as_str(),
            mutation.caller_service.as_str(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        mutation.status,
        RuntimeInvocationStatus::UnknownExecutionState
    );
    assert_eq!(
        mutation.terminal_error_code,
        Some(chatos_mcp_service::MCP_ERROR_UNKNOWN_EXECUTION_STATE)
    );
}

#[tokio::test]
async fn restart_recovery_closes_invocation_whose_batch_is_missing() {
    let store = RuntimeInvocationStore::memory();
    let mut queued = record();
    queued.status = RuntimeInvocationStatus::Queued;
    queued.started_at_unix_ms = None;
    store.register(queued.clone()).await.unwrap();

    assert!(store.recover_after_restart(&queued, false).await.unwrap());
    let recovered = store
        .get_for_caller(
            queued.invocation_id.as_str(),
            queued.caller_service.as_str(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, RuntimeInvocationStatus::Failed);
    assert!(recovered
        .terminal_error_message
        .as_deref()
        .is_some_and(|message| message.contains("durable tool batch")));
}
