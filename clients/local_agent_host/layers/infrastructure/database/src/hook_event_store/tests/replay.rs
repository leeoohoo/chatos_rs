#[tokio::test]
async fn event_and_invocation_resume_without_repeating_completed_work() {
    let store = SqliteClientStorage::connect_memory().await.expect("store");
    let mut large_event = event();
    large_event.event_id = "hook-event-large".to_string();
    large_event.sequence = 5_000_000_000;
    store
        .prepare_hook_event(&large_event, 5)
        .await
        .expect("large event");
    assert!(store
        .get_hook_event("user-1", "SessionStart", "conversation-1", 5_000_000_000,)
        .await
        .expect("get large event")
        .is_some());
    let event = event();
    let prepared = store.prepare_hook_event(&event, 10).await.expect("event");
    assert_eq!(prepared.status, "pending");
    assert!(store
        .list_pending_hook_events("user-1", 10)
        .await
        .expect("pending events")
        .contains(&prepared));
    assert!(store
        .has_hook_event("user-1", "SessionStart", "conversation-1")
        .await
        .expect("event exists"));
    assert_eq!(
        store
            .get_hook_event("user-1", "SessionStart", "conversation-1", 1)
            .await
            .expect("get event")
            .expect("event record")
            .spec,
        event
    );
    let invocation = LocalHookInvocationSpec {
        invocation_id: "hook-invocation-1".to_string(),
        event_id: event.event_id.clone(),
        bundle_id: "bundle-1".to_string(),
        hook_id: "hook-1".to_string(),
        is_async: false,
    };
    let first = store
        .begin_hook_invocation(&invocation, 20)
        .await
        .expect("begin");
    assert_eq!(first.attempt, 1);
    let resumed = store
        .begin_hook_invocation(&invocation, 30)
        .await
        .expect("resume");
    assert_eq!(resumed.attempt, 2);
    let completion = LocalHookInvocationCompletion {
        status: "succeeded".to_string(),
        exit_code: Some(0),
        timed_out: false,
        duration_ms: 12,
        stdout_sha256: Some("b".repeat(64)),
        stdout_bytes: 2,
        stdout_overflow: false,
        stderr_sha256: None,
        stderr_bytes: 0,
        stderr_overflow: false,
        stderr_summary: None,
        additional_context: Some("context".to_string()),
        stdout_spill: None,
        stderr_spill: None,
    };
    store
        .complete_hook_invocation(&invocation.invocation_id, 2, &completion, 40)
        .await
        .expect("complete invocation");
    let replay = store
        .begin_hook_invocation(&invocation, 50)
        .await
        .expect("completed replay");
    assert_eq!(replay.attempt, 2);
    assert_eq!(replay.status, "succeeded");
    let completed = store
        .complete_hook_event(&event.event_id, Some("context"), 60)
        .await
        .expect("complete event");
    assert_eq!(completed.status, "completed");
    assert!(!store
        .list_pending_hook_events("user-1", 10)
        .await
        .expect("completed events are not pending")
        .iter()
        .any(|record| record.spec.event_id == event.event_id));
    assert_eq!(
        store
            .prepare_hook_event(&event, 70)
            .await
            .expect("event replay")
            .additional_context
            .as_deref(),
        Some("context")
    );
}
