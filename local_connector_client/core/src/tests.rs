// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::history::CommandHistoryRecorder;
use crate::mcp::provider::{
    call_builtin_compatible_local_tool, local_mcp_builtin_compatible_tools,
};
use crate::mcp::service::handle_mcp_body;
use crate::mcp::tools::{code_maintainer_structured_result, local_browser_tools_service_for_root};
use crate::relay::RelayRequest;
use crate::sandbox::workspace::{
    local_sandbox_baseline_workspace, prepare_local_sandbox_workspace,
};
use crate::terminal::controller::{
    local_terminal_controller_context_for_root, LocalConnectorTerminalControllerStore,
};
use crate::terminal::guard::{
    normalize_path_for_guard, path_is_inside_root, sanitize_terminal_command_line,
    validate_local_terminal_command, validate_local_terminal_directory_change,
};
use crate::workspace::paths::resolve_request_workspace_path;

fn temp_test_dir(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("chatos-local-connector-{name}-{}", Uuid::new_v4()));
    fs::create_dir_all(path.as_path()).expect("create temp test dir");
    path
}

fn test_relay_request(workspace_id: &str) -> RelayRequest {
    RelayRequest {
        _message_type: "sandbox_request".to_string(),
        request_id: "req-test".to_string(),
        owner_user_id: Some("user-test".to_string()),
        device_id: Some("device-test".to_string()),
        workspace_id: workspace_id.to_string(),
        method: Some("POST".to_string()),
        path: Some("/api/sandboxes/leases".to_string()),
        headers: BTreeMap::new(),
        body: json!({}),
    }
}

fn test_workspace(root: &Path) -> WorkspaceState {
    WorkspaceState {
        id: "workspace-test".to_string(),
        absolute_root: fs::canonicalize(root).expect("canonical root"),
        alias: "workspace".to_string(),
        fingerprint: "fingerprint-test".to_string(),
    }
}

fn request_with_cwd(cwd: &str) -> RelayRequest {
    let mut request = test_relay_request("workspace-test");
    request
        .headers
        .insert("x-local-connector-cwd".to_string(), cwd.to_string());
    request
}

fn request_with_cwd_and_builtin_kinds(cwd: &str, kinds: &str) -> RelayRequest {
    let mut request = request_with_cwd(cwd);
    request.headers.insert(
        LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        kinds.to_string(),
    );
    request
}

fn test_state_with_workspace(workspace: WorkspaceState) -> LocalState {
    LocalState {
        workspaces: vec![workspace],
        ..LocalState::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn local_mcp_exposes_builtin_compatible_tools_and_project_relative_args() {
    let root = temp_test_dir("builtin-compatible");
    let project = root.join("apps").join("web");
    fs::create_dir_all(project.as_path()).expect("create project");
    fs::write(project.join("package.json"), "{\"name\":\"web\"}\n").expect("write package");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_workspace(workspace);
    let request = request_with_cwd_and_builtin_kinds(
        "apps/web",
        "CodeMaintainerRead,CodeMaintainerWrite,TerminalController,BrowserTools",
    );
    let recorder = CommandHistoryRecorder {
        state_path: root.join("state.json"),
        state: Arc::new(RwLock::new(state.clone())),
    };

    let tools = local_mcp_builtin_compatible_tools(&request, &state).expect("list tools");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    assert!(names.contains("read_file_raw"));
    assert!(names.contains("list_dir"));
    assert!(names.contains("write_file"));
    assert!(names.contains("execute_command"));
    assert!(names.contains("get_recent_logs"));
    assert!(names.contains("process"));
    assert!(names.contains("process_list"));
    assert!(names.contains("process_poll"));
    assert!(names.contains("process_log"));
    assert!(names.contains("process_wait"));
    assert!(names.contains("process_write"));
    assert!(names.contains("process_kill"));
    assert!(!names.contains("local_fs_read"));
    assert!(!names.contains("local_terminal_exec"));
    let browser_service =
        local_browser_tools_service_for_root(project.as_path(), &request).expect("browser service");
    let browser_names = browser_service
        .list_tools()
        .into_iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
        .collect::<BTreeSet<_>>();
    if browser_names.contains("browser_navigate") {
        assert!(names.contains("browser_navigate"));
        assert!(names.contains("browser_snapshot"));
        assert!(names.contains("browser_inspect"));
        assert!(!names.contains("browser_vision"));
    }

    let mut legacy_request =
        request_with_cwd_and_builtin_kinds("apps/web", "CodeMaintainerRead,CodeMaintainerWrite");
    legacy_request.body = json!({
        "jsonrpc": "2.0",
        "id": "legacy-tool",
        "method": "tools/call",
        "params": {
            "name": "local_fs_read",
            "arguments": { "path": "package.json" }
        }
    });
    let legacy_response = handle_mcp_body(&legacy_request, &state, &recorder)
        .await
        .expect("legacy tool response");
    assert_eq!(
        legacy_response
            .pointer("/error/code")
            .and_then(Value::as_i64),
        Some(-32000)
    );

    let read = call_builtin_compatible_local_tool(
        &request,
        &state,
        "read_file_raw",
        json!({ "path": "package.json", "with_line_numbers": false }),
        &recorder,
    )
    .await
    .expect("read call")
    .expect("read result");
    let structured = code_maintainer_structured_result(read);
    assert_eq!(
        structured.get("path").and_then(Value::as_str),
        Some("package.json")
    );
    assert_eq!(
        structured.get("content").and_then(Value::as_str),
        Some("{\"name\":\"web\"}\n")
    );

    let listed = call_builtin_compatible_local_tool(
        &request,
        &state,
        "list_dir",
        json!({
            "path": "local://connector/device-test/workspace-test/apps/web",
            "max_entries": 20
        }),
        &recorder,
    )
    .await
    .expect("list call")
    .expect("list result");
    let structured = code_maintainer_structured_result(listed);
    assert!(structured
        .get("entries")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .any(|entry| entry.get("path").and_then(Value::as_str) == Some("package.json")));

    let cwd_command = if cfg!(windows) { "cd" } else { "pwd" };
    let executed = call_builtin_compatible_local_tool(
        &request,
        &state,
        "execute_command",
        json!({ "path": ".", "common": cwd_command, "background": false }),
        &recorder,
    )
    .await
    .expect("execute call")
    .expect("execute result");
    let structured = code_maintainer_structured_result(executed);
    assert_eq!(
        structured.get("terminal_reused").and_then(Value::as_bool),
        Some(!cfg!(windows))
    );
    let stdout = structured
        .get("stdout")
        .or_else(|| structured.get("output"))
        .and_then(Value::as_str)
        .unwrap()
        .replace('\\', "/");
    assert!(stdout.contains("apps/web"));

    if !cfg!(windows) {
        let exported = call_builtin_compatible_local_tool(
            &request,
            &state,
            "execute_command",
            json!({ "path": ".", "common": "export CHATO_LOCAL_REUSE_TEST=ok", "background": false }),
            &recorder,
        )
        .await
        .expect("export call")
        .expect("export result");
        let structured = code_maintainer_structured_result(exported);
        assert_eq!(
            structured.get("terminal_reused").and_then(Value::as_bool),
            Some(true)
        );

        let echoed = call_builtin_compatible_local_tool(
            &request,
            &state,
            "execute_command",
            json!({ "path": ".", "common": "echo $CHATO_LOCAL_REUSE_TEST", "background": false }),
            &recorder,
        )
        .await
        .expect("echo call")
        .expect("echo result");
        let structured = code_maintainer_structured_result(echoed);
        assert_eq!(
            structured
                .get("stdout")
                .or_else(|| structured.get("output"))
                .and_then(Value::as_str)
                .unwrap()
                .trim(),
            "ok"
        );
    }

    if !cfg!(windows) {
        let processes = call_builtin_compatible_local_tool(
            &request,
            &state,
            "process_list",
            json!({ "include_exited": true, "limit": 5 }),
            &recorder,
        )
        .await
        .expect("process list call")
        .expect("process list result");
        let structured = code_maintainer_structured_result(processes);
        assert!(structured
            .get("processes")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|process| process
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| command.contains("task terminal shell"))));

        let recent_logs = call_builtin_compatible_local_tool(
            &request,
            &state,
            "get_recent_logs",
            json!({ "per_terminal_limit": 20, "terminal_limit": 5 }),
            &recorder,
        )
        .await
        .expect("recent logs call")
        .expect("recent logs result");
        let structured = code_maintainer_structured_result(recent_logs);
        assert!(structured
            .get("terminals")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|terminal| terminal
                .get("logs")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .any(|log| log.get("content").and_then(Value::as_str) == Some("pwd"))));
    }

    let context = local_terminal_controller_context_for_root(
        project.as_path(),
        &request,
        DEFAULT_TERMINAL_EXEC_TIMEOUT_MS,
    );
    LocalConnectorTerminalControllerStore
        .kill_sessions_for_context(context)
        .await
        .expect("cleanup local shell");
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[tokio::test(flavor = "multi_thread")]
async fn local_mcp_without_selected_builtin_kinds_exposes_no_tools() {
    let root = temp_test_dir("no-selected-tools");
    let project = root.join("apps").join("web");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_workspace(workspace);
    let request = request_with_cwd("apps/web");

    let tools = local_mcp_builtin_compatible_tools(&request, &state).expect("list tools");
    assert!(tools.is_empty());

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[tokio::test(flavor = "multi_thread")]
async fn local_mcp_respects_selected_builtin_kind_header() {
    let root = temp_test_dir("selected-tools");
    let project = root.join("apps").join("web");
    fs::create_dir_all(project.as_path()).expect("create project");
    fs::write(project.join("package.json"), "{\"name\":\"web\"}\n").expect("write package");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_workspace(workspace);
    let mut request = request_with_cwd_and_builtin_kinds("apps/web", "CodeMaintainerRead");
    let recorder = CommandHistoryRecorder {
        state_path: root.join("state.json"),
        state: Arc::new(RwLock::new(state.clone())),
    };

    let tools = local_mcp_builtin_compatible_tools(&request, &state).expect("list tools");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    assert!(names.contains("read_file_raw"));
    assert!(names.contains("list_dir"));
    assert!(!names.contains("write_file"));
    assert!(!names.contains("execute_command"));
    assert!(!names.contains("browser_navigate"));

    request.body = json!({
        "jsonrpc": "2.0",
        "id": "blocked-write",
        "method": "tools/call",
        "params": {
            "name": "write_file",
            "arguments": { "path": "package.json", "content": "{}\n" }
        }
    });
    let response = handle_mcp_body(&request, &state, &recorder)
        .await
        .expect("blocked write response");
    assert_eq!(
        response.pointer("/error/code").and_then(Value::as_i64),
        Some(-32000)
    );

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[tokio::test(flavor = "multi_thread")]
async fn local_mcp_lifecycle_starts_and_cleans_task_terminal() {
    let root = temp_test_dir("lifecycle-terminal");
    let project = root.join("apps").join("web");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_workspace(workspace);
    let recorder = CommandHistoryRecorder {
        state_path: root.join("state.json"),
        state: Arc::new(RwLock::new(state.clone())),
    };
    let mut request = request_with_cwd_and_builtin_kinds("apps/web", "TerminalController");
    request
        .headers
        .insert("x-task-runner-task-id".to_string(), "task-test".to_string());

    request.body = json!({
        "jsonrpc": "2.0",
        "id": "terminal-start",
        "method": "local_connector/terminal/start",
        "params": { "path": "." }
    });
    let started = handle_mcp_body(&request, &state, &recorder)
        .await
        .expect("start lifecycle terminal");
    assert_eq!(
        started.pointer("/result/status").and_then(Value::as_str),
        Some("running")
    );
    let started_terminal_id = started
        .pointer("/result/terminal_id")
        .and_then(Value::as_str)
        .expect("started terminal id")
        .to_string();

    let executed = call_builtin_compatible_local_tool(
        &request,
        &state,
        "execute_command",
        json!({ "path": ".", "common": "echo lifecycle", "background": false }),
        &recorder,
    )
    .await
    .expect("execute on lifecycle shell")
    .expect("execute result");
    let structured = code_maintainer_structured_result(executed);
    assert_eq!(
        structured.get("terminal_reused").and_then(Value::as_bool),
        Some(!cfg!(windows))
    );
    if !cfg!(windows) {
        assert_eq!(
            structured.get("terminal_id").and_then(Value::as_str),
            Some(started_terminal_id.as_str())
        );
    }

    let listed = call_builtin_compatible_local_tool(
        &request,
        &state,
        "process_list",
        json!({ "include_exited": false, "limit": 10 }),
        &recorder,
    )
    .await
    .expect("process list call")
    .expect("process list result");
    let structured = code_maintainer_structured_result(listed);
    assert!(structured
        .get("processes")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .any(|process| process
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains("task terminal shell"))));

    request.body = json!({
        "jsonrpc": "2.0",
        "id": "terminal-cleanup",
        "method": "local_connector/terminal/cleanup",
        "params": {}
    });
    let cleanup = handle_mcp_body(&request, &state, &recorder)
        .await
        .expect("cleanup lifecycle terminal");
    assert_eq!(
        cleanup.pointer("/result/ok").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        cleanup.pointer("/result/total").and_then(Value::as_u64),
        Some(if cfg!(windows) { 2 } else { 1 })
    );

    let listed = call_builtin_compatible_local_tool(
        &request,
        &state,
        "process_list",
        json!({ "include_exited": true, "limit": 10 }),
        &recorder,
    )
    .await
    .expect("process list call after cleanup")
    .expect("process list result after cleanup");
    let structured = code_maintainer_structured_result(listed);
    assert_eq!(
        structured
            .get("processes")
            .and_then(Value::as_array)
            .unwrap()
            .len(),
        0
    );

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn local_connector_path_at_current_cwd_resolves_to_project_root() {
    let root = temp_test_dir("path-local-uri");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let resolved = resolve_request_workspace_path(
        &workspace,
        &request,
        "local://connector/device-test/workspace-test/learn/applocations/react-fs-explorer",
    )
    .expect("resolve local connector project path");

    assert_eq!(
        normalize_path_for_guard(resolved.as_path()),
        normalize_path_for_guard(
            fs::canonicalize(project.as_path())
                .expect("canonical project")
                .as_path()
        )
    );
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn absolute_workspace_path_at_current_cwd_resolves_to_project_root() {
    let root = temp_test_dir("path-absolute-project");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let resolved =
        resolve_request_workspace_path(&workspace, &request, project.to_string_lossy().as_ref())
            .expect("resolve absolute project path");

    assert_eq!(
        normalize_path_for_guard(resolved.as_path()),
        normalize_path_for_guard(
            fs::canonicalize(project.as_path())
                .expect("canonical project")
                .as_path()
        )
    );
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn workspace_root_absolute_path_is_clamped_to_current_project_cwd() {
    let root = temp_test_dir("path-absolute-root");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let resolved = resolve_request_workspace_path(
        &workspace,
        &request,
        workspace.absolute_root.to_string_lossy().as_ref(),
    )
    .expect("resolve absolute workspace root path");

    assert_eq!(
        normalize_path_for_guard(resolved.as_path()),
        normalize_path_for_guard(
            fs::canonicalize(project.as_path())
                .expect("canonical project")
                .as_path()
        )
    );
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn workspace_absolute_path_outside_current_project_is_rejected() {
    let root = temp_test_dir("path-absolute-outside");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    let sibling = root.join("learn").join("other-project");
    fs::create_dir_all(project.as_path()).expect("create project");
    fs::create_dir_all(sibling.as_path()).expect("create sibling");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let err =
        resolve_request_workspace_path(&workspace, &request, sibling.to_string_lossy().as_ref())
            .expect_err("sibling project should be outside current project cwd");

    assert!(err.to_string().contains("outside current local project"));
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn relative_workspace_path_matching_cwd_is_not_duplicated() {
    let root = temp_test_dir("path-relative-cwd");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    let package_json = project.join("package.json");
    fs::create_dir_all(project.as_path()).expect("create project");
    fs::write(package_json.as_path(), "{}").expect("write package");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let resolved = resolve_request_workspace_path(
        &workspace,
        &request,
        "learn/applocations/react-fs-explorer/package.json",
    )
    .expect("resolve workspace-relative file path");

    assert_eq!(
        normalize_path_for_guard(resolved.as_path()),
        normalize_path_for_guard(
            fs::canonicalize(package_json.as_path())
                .expect("canonical package")
                .as_path()
        )
    );
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn prepare_local_sandbox_workspace_clears_existing_run_copy() {
    let root = temp_test_dir("workspace-copy");
    let workspace_root = root.join("project");
    fs::create_dir_all(workspace_root.as_path()).expect("create project root");
    fs::write(workspace_root.join("keep.txt"), "current").expect("write project file");
    fs::create_dir_all(workspace_root.join(".chatos").join("task-runner"))
        .expect("create internal dir");
    fs::write(
        workspace_root
            .join(".chatos")
            .join("task-runner")
            .join("skip.txt"),
        "internal",
    )
    .expect("write internal file");

    let run_workspace = workspace_root
        .join(".chatos")
        .join("task-runner")
        .join("runs")
        .join("run-test")
        .join("input")
        .join("workspace");
    let baseline_workspace =
        local_sandbox_baseline_workspace(run_workspace.as_path()).expect("baseline path");
    fs::create_dir_all(run_workspace.as_path()).expect("create run workspace");
    fs::create_dir_all(baseline_workspace.as_path()).expect("create baseline workspace");
    fs::write(run_workspace.join("stale.txt"), "old").expect("write stale run file");
    fs::write(baseline_workspace.join("stale.txt"), "old").expect("write stale baseline file");

    let workspace = WorkspaceState {
        id: "workspace-test".to_string(),
        absolute_root: fs::canonicalize(workspace_root.as_path()).expect("canonical root"),
        alias: "project".to_string(),
        fingerprint: "fingerprint-test".to_string(),
    };
    let state = LocalState {
        workspaces: vec![workspace],
        ..LocalState::default()
    };
    prepare_local_sandbox_workspace(
        &test_relay_request("workspace-test"),
        &state,
        &json!({ "run_workspace": run_workspace.to_string_lossy() }),
    )
    .expect("prepare workspace");

    assert!(run_workspace.join("keep.txt").is_file());
    assert!(baseline_workspace.join("keep.txt").is_file());
    assert!(!run_workspace.join("stale.txt").exists());
    assert!(!baseline_workspace.join("stale.txt").exists());
    assert!(!run_workspace.join(".chatos").exists());
    assert!(!baseline_workspace.join(".chatos").exists());

    fs::remove_dir_all(root.as_path()).expect("cleanup temp test dir");
}

#[test]
fn local_terminal_directory_guard_allows_descendants_and_blocks_escape() {
    let root = temp_test_dir("terminal-guard");
    let project = root.join("project");
    let child = project.join("child");
    fs::create_dir_all(child.as_path()).expect("create child");
    let project = fs::canonicalize(project.as_path()).expect("canonical project");
    let mut current = project.clone();

    assert!(
        validate_local_terminal_directory_change("cd child", project.as_path(), &mut current,)
            .is_none()
    );
    assert!(path_is_inside_root(current.as_path(), project.as_path()));

    assert!(
        validate_local_terminal_directory_change("cd ..", project.as_path(), &mut current,)
            .is_none()
    );
    assert_eq!(
        normalize_path_for_guard(current.as_path()),
        normalize_path_for_guard(project.as_path())
    );

    let blocked =
        validate_local_terminal_directory_change("cd ..", project.as_path(), &mut current);
    assert_eq!(
        blocked.as_deref(),
        Some("Blocked: cannot leave terminal workspace.")
    );

    let blocked_root =
        validate_local_terminal_directory_change("cd /", project.as_path(), &mut current);
    assert_eq!(
        blocked_root.as_deref(),
        Some("Blocked: cannot leave terminal workspace.")
    );

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn local_terminal_directory_guard_blocks_dynamic_and_pushd() {
    let root = temp_test_dir("terminal-guard-dynamic");
    let project = root.join("project");
    fs::create_dir_all(project.as_path()).expect("create project");
    let project = fs::canonicalize(project.as_path()).expect("canonical project");
    let mut current = project.clone();

    assert!(
        validate_local_terminal_directory_change("cd $HOME", project.as_path(), &mut current)
            .is_some()
    );
    assert!(
        validate_local_terminal_directory_change("pushd .", project.as_path(), &mut current)
            .is_some()
    );

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn local_terminal_directory_guard_blocks_ansi_wrapped_escape() {
    let root = temp_test_dir("terminal-guard-ansi");
    let project = root.join("project");
    let outside = root.join("outside");
    fs::create_dir_all(project.as_path()).expect("create project");
    fs::create_dir_all(outside.as_path()).expect("create outside");
    let project = fs::canonicalize(project.as_path()).expect("canonical project");
    let outside = fs::canonicalize(outside.as_path()).expect("canonical outside");
    let mut current = project.clone();
    let command = format!("\x1b[200~cd {}\x1b[201~", outside.display());
    let sanitized = sanitize_terminal_command_line(command.as_str());
    assert_eq!(sanitized, format!("cd {}", outside.display()));
    assert!(validate_local_terminal_directory_change(
        sanitized.as_str(),
        project.as_path(),
        &mut current,
    )
    .is_some());

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn local_terminal_command_guard_blocks_obvious_outside_path_arguments() {
    let root = temp_test_dir("terminal-guard-path-args");
    let project = root.join("project");
    let child = project.join("child");
    let outside = root.join("outside");
    fs::create_dir_all(child.as_path()).expect("create child");
    fs::create_dir_all(outside.as_path()).expect("create outside");
    fs::write(child.join("file.txt"), "ok").expect("write child file");
    fs::write(outside.join("secret.txt"), "nope").expect("write outside file");
    let project = fs::canonicalize(project.as_path()).expect("canonical project");
    let outside = fs::canonicalize(outside.as_path()).expect("canonical outside");
    let mut current = project.clone();

    assert!(
        validate_local_terminal_command("ls child", project.as_path(), &mut current,).is_none()
    );
    assert!(
        validate_local_terminal_command("cat child/file.txt", project.as_path(), &mut current,)
            .is_none()
    );
    assert!(validate_local_terminal_command("ls /", project.as_path(), &mut current).is_some());
    assert!(validate_local_terminal_command(
        format!("cat {}", outside.join("secret.txt").display()).as_str(),
        project.as_path(),
        &mut current,
    )
    .is_some());
    assert!(validate_local_terminal_command(
        "cat ../outside/secret.txt",
        project.as_path(),
        &mut current,
    )
    .is_some());

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}
