#[tokio::test]
async fn plugin_hook_history_is_owner_scoped_safe_and_stably_paginated() {
    let store = SqliteClientStorage::connect_memory().await.expect("store");
    for (bundle_id, plugin_id) in [
        ("bundle-history", "plugin-history"),
        ("bundle-other", "plugin-other"),
    ] {
        store
            .put_plugin_hook_bundle(
                &command(&format!("put-{bundle_id}")),
                &hook_bundle(bundle_id, plugin_id),
                1,
            )
            .await
            .expect("put bundle");
    }

    for (suffix, bundle_id, updated_at, status) in [
        ("older", "bundle-history", 30, "failed"),
        ("newer", "bundle-history", 40, "succeeded"),
        ("other", "bundle-other", 50, "succeeded"),
    ] {
        let mut hook_event = event();
        hook_event.event_id = format!("hook-event-{suffix}");
        hook_event.subject_id = format!("conversation-{suffix}");
        hook_event.sequence = updated_at as u64;
        hook_event.active_bundle_ids = vec![bundle_id.to_string()];
        store
            .prepare_hook_event(&hook_event, updated_at - 10)
            .await
            .expect("prepare event");
        let invocation = LocalHookInvocationSpec {
            invocation_id: format!("hook-invocation-{suffix}"),
            event_id: hook_event.event_id,
            bundle_id: bundle_id.to_string(),
            hook_id: "hook-1".to_string(),
            is_async: suffix == "newer",
        };
        store
            .begin_hook_invocation(&invocation, updated_at - 5)
            .await
            .expect("begin invocation");
        store
            .complete_hook_invocation(
                &invocation.invocation_id,
                1,
                &LocalHookInvocationCompletion {
                    status: status.to_string(),
                    exit_code: Some(if status == "succeeded" { 0 } else { 7 }),
                    timed_out: status == "failed",
                    duration_ms: updated_at as u64,
                    stdout_sha256: Some("f".repeat(64)),
                    stdout_bytes: 12,
                    stdout_overflow: false,
                    stderr_sha256: Some("0".repeat(64)),
                    stderr_bytes: 34,
                    stderr_overflow: true,
                    stderr_summary: Some("secret stderr must not cross IPC".to_string()),
                    additional_context: Some("secret model context".to_string()),
                    stdout_spill: None,
                    stderr_spill: None,
                },
                updated_at,
            )
            .await
            .expect("complete invocation");
    }

    let first = store
        .list_plugin_hook_invocations("user-1", "plugin-history", None, None, 1)
        .await
        .expect("first page");
    assert_eq!(first.invocations.len(), 1);
    assert_eq!(first.invocations[0].invocation_id, "hook-invocation-newer");
    assert!(first.invocations[0].is_async);
    let encoded = serde_json::to_value(&first.invocations[0]).expect("diagnostic json");
    for forbidden in [
        "stderr_summary",
        "additional_context",
        "stdout_sha256",
        "stderr_sha256",
        "subject_id",
        "payload_json",
    ] {
        assert!(
            encoded.get(forbidden).is_none(),
            "leaked field: {forbidden}"
        );
    }
    let second = store
        .list_plugin_hook_invocations(
            "user-1",
            "plugin-history",
            first.next_before_updated_at_unix_ms,
            first.next_before_invocation_id.as_deref(),
            1,
        )
        .await
        .expect("second page");
    assert_eq!(second.invocations[0].invocation_id, "hook-invocation-older");
    assert!(second.next_before_invocation_id.is_none());
    assert!(store
        .list_plugin_hook_invocations("user-2", "plugin-history", None, None, 10)
        .await
        .expect("other owner")
        .invocations
        .is_empty());

    let plan = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT i.invocation_id, e.event_type \
             FROM local_hook_invocations i INNER JOIN local_hook_events e \
             ON e.event_id = i.event_id AND e.owner_user_id = i.owner_user_id \
             WHERE i.owner_user_id = ? AND i.plugin_id = ? \
             ORDER BY i.updated_at_unix_ms DESC, i.invocation_id DESC LIMIT ?",
    )
    .bind("user-1")
    .bind("plugin-history")
    .bind(21_i64)
    .fetch_all(&store.pool)
    .await
    .expect("history query plan")
    .into_iter()
    .map(|row| row.get::<String, _>("detail"))
    .collect::<Vec<_>>()
    .join("\n");
    assert!(
        plan.contains("local_hook_invocations_plugin_history"),
        "{plan}"
    );
    assert!(!plan.contains("TEMP B-TREE"), "{plan}");
}
