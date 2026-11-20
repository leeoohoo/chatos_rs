use super::*;
use tempfile::TempDir;

fn fixture(command: &str, timeout_ms: u64) -> (TempDir, LocalHookInvocation) {
    let root = tempfile::tempdir().expect("root");
    let installation = root.path().join("plugin");
    std::fs::create_dir_all(&installation).expect("plugin root");
    let hook = LocalPluginHookDefinition {
        hook_id: "hook-1".to_string(),
        event: "SessionStart".to_string(),
        matcher: None,
        command: command.to_string(),
        is_async: false,
        timeout_ms,
        additional_context_limit: 65_536,
    };
    let bundle = LocalPluginHookBundleSpec {
        bundle_id: "bundle-1".to_string(),
        owner_user_id: "user-1".to_string(),
        plugin_id: "plugin-1".to_string(),
        release_id: "release-1".to_string(),
        component_key: "hooks-1".to_string(),
        artifact_sha256: "a".repeat(64),
        content_sha256: "b".repeat(64),
        normalized_hook_sha256: "c".repeat(64),
        command_sha256: "d".repeat(64),
        permission_snapshot_sha256: "e".repeat(64),
        runner_policy_version: 1,
        installation_root: installation.display().to_string(),
        permissions: vec!["process.spawn".to_string()],
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
async fn seatbelt_runner_returns_only_stdout_protocol_data() {
    let (root, invocation) = fixture(
        r#"printf '%s' '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"mode loaded"}}'; printf 'diagnostic' >&2"#,
        2_000,
    );
    let runner = MacOsSeatbeltHookRunner::new(root.path().join("data")).expect("runner");
    let result = runner.run(invocation).await.expect("run");
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.timed_out);
    assert_eq!(result.stderr, b"diagnostic");
    let output: serde_json::Value = serde_json::from_slice(&result.stdout).expect("json");
    assert_eq!(
        output["hookSpecificOutput"]["additionalContext"],
        "mode loaded"
    );
}

#[tokio::test]
async fn timeout_terminates_the_hook_process_group() {
    let (root, invocation) = fixture("sleep 30", 100);
    let runner = MacOsSeatbeltHookRunner::new(root.path().join("data")).expect("runner");
    let started = Instant::now();
    let result = runner.run(invocation).await.expect("run");
    assert!(result.timed_out);
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn node_hook_can_write_plugin_data_but_not_workspace_or_network() {
    let (root, mut invocation) = fixture("node \"$PLUGIN_ROOT/security-probe.js\"", 5_000);
    let installation = PathBuf::from(&invocation.bundle.installation_root);
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    std::fs::write(workspace.join("readable.txt"), "workspace-visible").expect("workspace fixture");
    std::fs::write(
        installation.join("security-probe.js"),
        r#"
const fs = require('fs');
const net = require('net');
const path = require('path');
const outcome = {
  workspaceRead: fs.readFileSync(path.join(process.env.CLAUDE_PROJECT_DIR, 'readable.txt'), 'utf8'),
  pluginDataWrite: false,
  workspaceWriteBlocked: false,
  networkError: null,
};
fs.writeFileSync(path.join(process.env.PLUGIN_DATA, 'written.txt'), 'plugin-data-writable');
outcome.pluginDataWrite = true;
try {
  fs.writeFileSync(path.join(process.env.CLAUDE_PROJECT_DIR, 'forbidden.txt'), 'must-not-exist');
} catch (_) {
  outcome.workspaceWriteBlocked = true;
}

let finished = false;
function finish(networkError) {
  if (finished) return;
  finished = true;
  outcome.networkError = networkError;
  process.stdout.write(JSON.stringify(outcome));
}
const socket = net.connect({host: '127.0.0.1', port: 9});
socket.on('connect', () => { socket.destroy(); finish('connected'); });
socket.on('error', (error) => finish(error.code || String(error)));
setTimeout(() => { socket.destroy(); finish('timeout'); }, 1000);
"#,
    )
    .expect("security probe");
    invocation.bundle.permissions = vec!["process.spawn".to_string(), "workspace.read".to_string()];
    invocation.hook = LocalPluginHookDefinition {
        hook_id: "hook-1".to_string(),
        event: "SessionStart".to_string(),
        matcher: None,
        command: "node \"$PLUGIN_ROOT/security-probe.js\"".to_string(),
        is_async: false,
        timeout_ms: 5_000,
        additional_context_limit: 65_536,
    };
    invocation.bundle.hooks = vec![invocation.hook.clone()];
    invocation.workspace_root = Some(workspace.clone());

    let data_root = root.path().join("data");
    let runner = MacOsSeatbeltHookRunner::new(data_root.clone()).expect("runner");
    let result = runner.run(invocation).await.expect("run");

    assert_eq!(
        result.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&result.stdout).expect("probe json");
    assert_eq!(outcome["workspaceRead"], "workspace-visible");
    assert_eq!(outcome["pluginDataWrite"], true);
    assert_eq!(outcome["workspaceWriteBlocked"], true);
    assert!(matches!(
        outcome["networkError"].as_str(),
        Some("EPERM" | "EACCES")
    ));
    assert!(!workspace.join("forbidden.txt").exists());
    let plugin_data = std::fs::read_dir(&data_root)
        .expect("data root")
        .next()
        .expect("plugin data directory")
        .expect("plugin data entry")
        .path();
    assert_eq!(
        std::fs::read_to_string(plugin_data.join("written.txt")).expect("plugin data"),
        "plugin-data-writable"
    );
}
