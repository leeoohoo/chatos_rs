#[tokio::test]
async fn large_hook_outputs_spill_outside_sqlite_and_expire() {
    let store = SqliteClientStorage::connect_memory().await.expect("store");
    let base_event = event();
    store
        .prepare_hook_event(&base_event, 10)
        .await
        .expect("event");
    let invocation = LocalHookInvocationSpec {
        invocation_id: "hook-invocation-large".to_string(),
        event_id: base_event.event_id.clone(),
        bundle_id: "bundle-large".to_string(),
        hook_id: "hook-large".to_string(),
        is_async: false,
    };
    store
        .begin_hook_invocation(&invocation, 20)
        .await
        .expect("begin");
    let stdout = vec![b'x'; OUTPUT_SPILL_THRESHOLD_BYTES + 1];
    let stderr = vec![b's'; OUTPUT_SPILL_THRESHOLD_BYTES + 2];
    let completion = LocalHookInvocationCompletion {
        status: "failed".to_string(),
        exit_code: Some(1),
        timed_out: false,
        duration_ms: 45,
        stdout_sha256: Some(format!("{:x}", Sha256::digest(&stdout))),
        stdout_bytes: stdout.len() as u64,
        stdout_overflow: false,
        stderr_sha256: Some(format!("{:x}", Sha256::digest(&stderr))),
        stderr_bytes: stderr.len() as u64,
        stderr_overflow: false,
        stderr_summary: Some("redacted diagnostic".to_string()),
        additional_context: None,
        stdout_spill: Some(chatos_local_agent_ports::LocalHookOutputSpill::new(
            stdout.clone(),
        )),
        stderr_spill: Some(chatos_local_agent_ports::LocalHookOutputSpill::new(
            stderr.clone(),
        )),
    };

    let completed = store
        .complete_hook_invocation(&invocation.invocation_id, 1, &completion, 100)
        .await
        .expect("complete");
    assert_eq!(completed.duration_ms, 45);
    assert_eq!(
        completed.stderr_summary.as_deref(),
        Some("redacted diagnostic")
    );
    let blobs = store
        .list_hook_output_blobs(&invocation.invocation_id)
        .await
        .expect("blobs");
    assert_eq!(blobs.len(), 2);
    assert_eq!(blobs[0].stream, "stderr");
    assert_eq!(blobs[1].stream, "stdout");
    assert!(!format!("{blobs:?}").contains(&"s".repeat(128)));
    let paths = blobs
        .iter()
        .map(|blob| store.hook_output_root.join(&blob.storage_ref))
        .collect::<Vec<_>>();
    assert_eq!(std::fs::read(&paths[0]).expect("stderr blob"), stderr);
    assert_eq!(std::fs::read(&paths[1]).expect("stdout blob"), stdout);

    let mut cleanup_event = event();
    cleanup_event.event_id = "hook-event-cleanup".to_string();
    cleanup_event.subject_id = "conversation-cleanup".to_string();
    cleanup_event.sequence = 2;
    store
        .prepare_hook_event(&cleanup_event, 30)
        .await
        .expect("cleanup event");
    let cleanup_invocation = LocalHookInvocationSpec {
        invocation_id: "hook-invocation-cleanup".to_string(),
        event_id: cleanup_event.event_id,
        bundle_id: "bundle-cleanup".to_string(),
        hook_id: "hook-cleanup".to_string(),
        is_async: false,
    };
    store
        .begin_hook_invocation(&cleanup_invocation, 40)
        .await
        .expect("cleanup begin");
    let cleanup_completion = LocalHookInvocationCompletion {
        status: "succeeded".to_string(),
        exit_code: Some(0),
        timed_out: false,
        duration_ms: 1,
        stdout_sha256: Some(format!("{:x}", Sha256::digest([]))),
        stdout_bytes: 0,
        stdout_overflow: false,
        stderr_sha256: None,
        stderr_bytes: 0,
        stderr_overflow: false,
        stderr_summary: None,
        additional_context: None,
        stdout_spill: None,
        stderr_spill: None,
    };
    store
        .complete_hook_invocation(
            &cleanup_invocation.invocation_id,
            1,
            &cleanup_completion,
            100 + OUTPUT_RETENTION_MS,
        )
        .await
        .expect("cleanup completion");

    assert!(store
        .list_hook_output_blobs(&invocation.invocation_id)
        .await
        .expect("expired blobs")
        .is_empty());
    assert!(paths.iter().all(|path| !path.exists()));
}
