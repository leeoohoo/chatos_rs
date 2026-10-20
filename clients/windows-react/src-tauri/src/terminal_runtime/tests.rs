use super::*;
use std::fs;
use tokio::time::{sleep, Duration};

fn scope(root: &std::path::Path, run_id: &str) -> TerminalScope {
    TerminalScope {
        run_id: run_id.into(),
        workspace_scope_id: "workspace-1".into(),
        project_root: fs::canonicalize(root).unwrap(),
    }
}

#[cfg(windows)]
fn print_command(value: &str) -> String {
    format!("[Console]::Out.Write('{value}')")
}

#[cfg(not(windows))]
fn print_command(value: &str) -> String {
    format!("printf '{value}'")
}

#[cfg(windows)]
fn delayed_command(value: &str, milliseconds: u64) -> String {
    format!("Start-Sleep -Milliseconds {milliseconds}; [Console]::Out.Write('{value}')")
}

#[cfg(not(windows))]
fn delayed_command(value: &str, milliseconds: u64) -> String {
    format!("sleep {}; printf '{value}'", milliseconds as f64 / 1_000.0)
}

#[cfg(windows)]
fn input_command() -> String {
    "$line = [Console]::In.ReadLine(); [Console]::Out.Write($line)".into()
}

#[cfg(not(windows))]
fn input_command() -> String {
    "IFS= read -r line; printf '%s' \"$line\"".into()
}

#[tokio::test]
async fn foreground_execution_returns_bounded_terminal_shape() {
    let directory = tempfile::tempdir().unwrap();
    let store = TerminalProcessStore::default();
    let scope = scope(directory.path(), "run-1");
    let result = store
        .execute(
            &scope,
            ExecuteRequest {
                command: print_command("hello"),
                cwd: scope.project_root.clone(),
                background: false,
                timeout_ms: Some(5_000),
            },
        )
        .await
        .unwrap();

    assert_eq!(result["success"], true);
    assert_eq!(result["output"], "hello");
    assert_eq!(result["path"], ".");
    assert_eq!(result["finished_by"], "exit");
    assert!(result["terminal_id"].as_str().is_some());
    store.reset().await;
}

#[tokio::test]
async fn background_process_is_run_scoped_and_supports_poll_wait_and_log() {
    let directory = tempfile::tempdir().unwrap();
    let store = TerminalProcessStore::default();
    let owner = scope(directory.path(), "run-1");
    let other = scope(directory.path(), "run-2");
    let started = store
        .execute(
            &owner,
            ExecuteRequest {
                command: delayed_command("finished", 200),
                cwd: owner.project_root.clone(),
                background: true,
                timeout_ms: None,
            },
        )
        .await
        .unwrap();
    let terminal_id = started["terminal_id"].as_str().unwrap();

    assert!(store.poll(&other, terminal_id, None, 20).await.is_err());
    let waited = store.wait(&owner, terminal_id, 5_000).await.unwrap();
    assert_eq!(waited["completed"], true);
    assert_eq!(waited["output"], "finished");
    let log = store.log(&owner, terminal_id, Some(0), 200).await.unwrap();
    assert!(log["output"].as_str().unwrap().contains("finished"));
    store.reset().await;
}

#[tokio::test]
async fn process_input_and_kill_are_confined_to_the_owning_run() {
    let directory = tempfile::tempdir().unwrap();
    let store = TerminalProcessStore::default();
    let owner = scope(directory.path(), "run-1");
    let started = store
        .execute(
            &owner,
            ExecuteRequest {
                command: input_command(),
                cwd: owner.project_root.clone(),
                background: true,
                timeout_ms: None,
            },
        )
        .await
        .unwrap();
    let terminal_id = started["terminal_id"].as_str().unwrap();
    let write = store
        .write(&owner, terminal_id, "accepted", true)
        .await
        .unwrap();
    assert_eq!(write["submit"], true);
    let waited = store.wait(&owner, terminal_id, 5_000).await.unwrap();
    assert_eq!(waited["output"], "accepted");

    let long_running = store
        .execute(
            &owner,
            ExecuteRequest {
                command: delayed_command("too-late", 10_000),
                cwd: owner.project_root.clone(),
                background: true,
                timeout_ms: None,
            },
        )
        .await
        .unwrap();
    let long_id = long_running["terminal_id"].as_str().unwrap();
    store.kill(&owner, long_id).await.unwrap();
    assert_eq!(
        store.poll(&owner, long_id, None, 20).await.unwrap()["busy"],
        false
    );
    store.reset().await;
}

#[tokio::test]
async fn foreground_timeout_terminates_the_owned_process_tree() {
    let directory = tempfile::tempdir().unwrap();
    let store = TerminalProcessStore::default();
    let scope = scope(directory.path(), "run-1");
    let result = store
        .execute(
            &scope,
            ExecuteRequest {
                command: delayed_command("too-late", 10_000),
                cwd: scope.project_root.clone(),
                background: false,
                timeout_ms: Some(1_000),
            },
        )
        .await
        .unwrap();

    assert_eq!(result["timed_out"], true);
    assert_eq!(result["busy"], false);
    assert_eq!(result["finished_by"], "timeout");
    store.reset().await;
}

#[tokio::test]
async fn reset_terminates_background_processes() {
    let directory = tempfile::tempdir().unwrap();
    let store = TerminalProcessStore::default();
    let scope = scope(directory.path(), "run-1");
    let started = store
        .execute(
            &scope,
            ExecuteRequest {
                command: delayed_command("too-late", 10_000),
                cwd: scope.project_root.clone(),
                background: true,
                timeout_ms: None,
            },
        )
        .await
        .unwrap();
    let terminal_id = started["terminal_id"].as_str().unwrap().to_owned();
    store.reset().await;
    sleep(Duration::from_millis(50)).await;
    assert!(store.poll(&scope, &terminal_id, None, 20).await.is_err());
}
