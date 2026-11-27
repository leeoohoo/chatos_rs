fn event() -> LocalHookEventSpec {
    let payload_json = "{}".to_string();
    LocalHookEventSpec {
        event_id: "hook-event-1".to_string(),
        owner_user_id: "user-1".to_string(),
        event_type: "SessionStart".to_string(),
        subject_id: "conversation-1".to_string(),
        sequence: 1,
        payload_sha256: format!("{:x}", Sha256::digest(payload_json.as_bytes())),
        payload_json,
        matcher_value: String::new(),
        active_bundle_ids: vec!["bundle-1".to_string()],
    }
}

fn hook_bundle(bundle_id: &str, plugin_id: &str) -> LocalPluginHookBundleSpec {
    LocalPluginHookBundleSpec {
        bundle_id: bundle_id.to_string(),
        owner_user_id: "user-1".to_string(),
        plugin_id: plugin_id.to_string(),
        release_id: "release-1".to_string(),
        component_key: format!("hooks-{plugin_id}"),
        artifact_sha256: "a".repeat(64),
        content_sha256: "b".repeat(64),
        normalized_hook_sha256: "c".repeat(64),
        command_sha256: "d".repeat(64),
        permission_snapshot_sha256: "e".repeat(64),
        runner_policy_version: 1,
        installation_root: format!("/plugins/{plugin_id}"),
        permissions: vec!["process.spawn".to_string()],
        task_policy: None,
        hooks: vec![LocalPluginHookDefinition {
            hook_id: "hook-1".to_string(),
            event: "SessionStart".to_string(),
            matcher: None,
            command: "node hook.js".to_string(),
            is_async: false,
            timeout_ms: 5_000,
            additional_context_limit: 65_536,
        }],
        enabled: true,
        trusted_at_unix_ms: 1,
    }
}

fn command(command_id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: command_id.to_string(),
        request_fingerprint: format!("fingerprint-{command_id}"),
        persist_receipt: true,
    }
}
