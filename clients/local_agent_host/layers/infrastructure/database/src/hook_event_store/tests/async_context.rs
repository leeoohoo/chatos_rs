#[tokio::test]
async fn async_context_claim_is_iteration_stable_and_interrupted_work_is_not_replayed() {
    let store = SqliteClientStorage::connect_memory().await.expect("store");
    let mut async_event = event();
    async_event.event_id = "hook-event-async".to_string();
    async_event.subject_id = "run-async".to_string();
    async_event.event_type = "Interrupt".to_string();
    async_event.payload_json = r#"{"session_id":"conversation-async"}"#.to_string();
    async_event.payload_sha256 =
        format!("{:x}", Sha256::digest(async_event.payload_json.as_bytes()));
    store
        .prepare_hook_event(&async_event, 10)
        .await
        .expect("async event");
    let completed_invocation = LocalHookInvocationSpec {
        invocation_id: "hook-invocation-async-completed".to_string(),
        event_id: async_event.event_id.clone(),
        bundle_id: "bundle-async".to_string(),
        hook_id: "hook-async".to_string(),
        is_async: true,
    };
    store
        .begin_hook_invocation(&completed_invocation, 20)
        .await
        .expect("begin async invocation");
    store
        .complete_hook_invocation(
            &completed_invocation.invocation_id,
            1,
            &LocalHookInvocationCompletion {
                status: "succeeded".to_string(),
                exit_code: Some(0),
                timed_out: false,
                duration_ms: 10,
                stdout_sha256: Some("a".repeat(64)),
                stdout_bytes: 2,
                stdout_overflow: false,
                stderr_sha256: None,
                stderr_bytes: 0,
                stderr_overflow: false,
                stderr_summary: None,
                additional_context: Some(r#"{"additional_context":"background"}"#.to_string()),
                stdout_spill: None,
                stderr_spill: None,
            },
            30,
        )
        .await
        .expect("complete async invocation");
    let first = store
        .claim_async_hook_contexts("user-1", "conversation-async", "delivery-1", 10, 40)
        .await
        .expect("claim async context");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].async_delivery_key, "delivery-1");
    assert_eq!(
        store
            .claim_async_hook_contexts("user-1", "conversation-async", "delivery-1", 10, 50,)
            .await
            .expect("same delivery replay"),
        first
    );
    assert!(store
        .claim_async_hook_contexts("user-1", "conversation-async", "delivery-2", 10, 60,)
        .await
        .expect("later delivery")
        .is_empty());

    let interrupted = LocalHookInvocationSpec {
        invocation_id: "hook-invocation-async-interrupted".to_string(),
        event_id: async_event.event_id,
        bundle_id: "bundle-async".to_string(),
        hook_id: "hook-interrupted".to_string(),
        is_async: true,
    };
    store
        .begin_hook_invocation(&interrupted, 70)
        .await
        .expect("begin interrupted invocation");
    assert_eq!(
        store
            .abandon_running_async_hook_invocations("user-1", 80)
            .await
            .expect("abandon interrupted async Hook"),
        1
    );
    let abandoned = store
        .begin_hook_invocation(&interrupted, 90)
        .await
        .expect("interrupted invocation is terminal");
    assert_eq!(abandoned.status, "failed");
    assert_eq!(abandoned.attempt, 1);
}
