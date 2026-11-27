use super::*;
use tempfile::TempDir;

fn fixture(command: &str, timeout_ms: u64) -> (TempDir, LocalHookInvocation) {
    let root = tempfile::tempdir().expect("root");
    let installation = root.path().join("plugin");
    std::fs::create_dir_all(&installation).expect("plugin root");
    std::fs::write(installation.join("plugin.txt"), "installed").expect("fixture");
    let hook = LocalPluginHookDefinition {
        hook_id: "SessionStart-0-0".into(),
        event: "SessionStart".into(),
        matcher: None,
        command: command.into(),
        is_async: false,
        timeout_ms,
        additional_context_limit: 65_536,
    };
    let bundle = LocalPluginHookBundleSpec {
        bundle_id: "bundle-1".into(),
        owner_user_id: "user-1".into(),
        plugin_id: "plugin-1".into(),
        release_id: "release-1".into(),
        component_key: "hooks-1".into(),
        artifact_sha256: "a".repeat(64),
        content_sha256: "b".repeat(64),
        normalized_hook_sha256: "c".repeat(64),
        command_sha256: "d".repeat(64),
        permission_snapshot_sha256: "e".repeat(64),
        runner_policy_version: 1,
        installation_root: installation.display().to_string(),
        permissions: vec!["process.spawn".into()],
        task_policy: None,
        hooks: vec![hook.clone()],
        enabled: true,
        trusted_at_unix_ms: 1,
    };
    (
        root,
        LocalHookInvocation {
            bundle,
            hook,
            stdin_json: br#"{"session_id":"session-1"}"#.to_vec(),
            workspace_root: None,
        },
    )
}

#[tokio::test]
async fn appcontainer_runner_executes_hook_with_bounded_stdio() {
    let (root, invocation) = fixture("echo hook-ok", 5_000);
    let runner = WindowsAppContainerHookRunner::new(root.path().join("hook-data")).expect("runner");
    let result = runner.run(invocation).await.expect("run");
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.timed_out);
    assert!(!result.stdout_overflow);
    assert!(!result.stderr_overflow);
    assert!(String::from_utf8_lossy(&result.stdout).contains("hook-ok"));
}

#[tokio::test]
async fn appcontainer_runner_reports_timeout_and_reclaims_process_tree() {
    let (root, invocation) = fixture("for /L %i in (1,1,100000000) do @rem", 100);
    let runner = WindowsAppContainerHookRunner::new(root.path().join("hook-data")).expect("runner");
    let result = runner.run(invocation).await.expect("timeout result");
    assert!(result.timed_out);
    assert!(result.exit_code.is_none());
}

#[tokio::test]
async fn appcontainer_runner_serializes_overlapping_installation_leases() {
    let (root, first) = fixture("echo first", 5_000);
    let mut second = first.clone();
    second.hook.command = "echo second".into();
    second.bundle.hooks = vec![second.hook.clone()];
    let runner = WindowsAppContainerHookRunner::new(root.path().join("hook-data")).expect("runner");
    let (first, second) = tokio::join!(runner.run(first), runner.run(second));
    assert_eq!(first.expect("first").exit_code, Some(0));
    assert_eq!(second.expect("second").exit_code, Some(0));
}
