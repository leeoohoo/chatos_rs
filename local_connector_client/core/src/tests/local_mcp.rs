// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::history::CommandHistoryRecorder;
use crate::mcp::provider::{
    call_builtin_compatible_local_tool, local_mcp_builtin_compatible_tools,
};
use crate::mcp::service::handle_mcp_body;
use crate::mcp::tools::{code_maintainer_structured_result, local_browser_tools_service_for_root};
use crate::terminal::controller::{
    local_terminal_controller_context_for_root, LocalConnectorTerminalControllerStore,
};

#[tokio::test(flavor = "multi_thread")]
async fn exposes_builtin_compatible_tools_and_project_relative_args() {
    let root = temp_test_dir("builtin-compatible");
    let project = root.join("apps").join("web");
    fs::create_dir_all(project.as_path()).expect("create project");
    fs::write(project.join("package.json"), "{\"name\":\"web\"}\n").expect("write package");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_full_control_workspace(workspace);
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
    assert!(names.contains("open_edit_session"));
    assert!(names.contains("stage_edit_batch"));
    assert!(names.contains("commit_edit_session"));
    assert!(names.contains("abort_edit_session"));
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
    let browser_service = local_browser_tools_service_for_root(project.as_path(), &request, false)
        .expect("browser service");
    let browser_names = browser_service
        .list_tools()
        .into_iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
        .collect::<BTreeSet<_>>();
    if browser_names.contains("browser_navigate") {
        assert!(names.contains("browser_tabs"));
        assert!(names.contains("browser_tab_new"));
        assert!(names.contains("browser_tab_switch"));
        assert!(names.contains("browser_tab_close"));
        assert!(names.contains("browser_navigate"));
        assert!(names.contains("browser_snapshot"));
        assert!(names.contains("browser_inspect"));
        assert!(names.contains("browser_upload"));
        assert!(names.contains("browser_download"));
        assert!(names.contains("browser_network"));
        assert!(names.contains("browser_network_request"));
        assert!(names.contains("browser_har_start"));
        assert!(names.contains("browser_har_stop"));
        assert!(names.contains("browser_websocket_start"));
        assert!(names.contains("browser_websocket_frames"));
        assert!(names.contains("browser_websocket_stop"));
        assert!(names.contains("browser_route_add"));
        assert!(names.contains("browser_route_list"));
        assert!(names.contains("browser_route_remove"));
        assert!(names.contains("browser_route_clear"));
        assert!(!names.contains("browser_cdp_command"));
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

    let opened = call_builtin_compatible_local_tool(
        &request,
        &state,
        "open_edit_session",
        json!({}),
        &recorder,
    )
    .await
    .expect("open edit session")
    .expect("open result");
    let session_id = code_maintainer_structured_result(opened)["result"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    call_builtin_compatible_local_tool(
        &request,
        &state,
        "stage_edit_batch",
        json!({
            "session_id": session_id.clone(),
            "operations": [{
                "kind": "write",
                "path": "session-write.txt",
                "content": "persisted",
                "expected_sha256": null
            }]
        }),
        &recorder,
    )
    .await
    .expect("stage edit batch")
    .expect("stage result");
    call_builtin_compatible_local_tool(
        &request,
        &state,
        "commit_edit_session",
        json!({ "session_id": session_id }),
        &recorder,
    )
    .await
    .expect("commit edit session")
    .expect("commit result");
    assert_eq!(
        fs::read_to_string(project.join("session-write.txt")).expect("read session write"),
        "persisted"
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
async fn code_maintainer_edit_sessions_are_isolated_between_parallel_mcp_runs() {
    let root = temp_test_dir("parallel-edit-sessions");
    let project = root.join("project");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_full_control_workspace(workspace);
    let recorder = CommandHistoryRecorder {
        state_path: root.join("state.json"),
        state: Arc::new(RwLock::new(state.clone())),
    };
    let mut first_request =
        request_with_cwd_and_builtin_kinds("project", "CodeMaintainerRead,CodeMaintainerWrite");
    first_request.headers.insert(
        "x-mcp-management-session-id".to_string(),
        "mcp-session-a".to_string(),
    );
    first_request.headers.insert(
        "x-mcp-management-run-id".to_string(),
        "task-run-a".to_string(),
    );
    let mut second_request = first_request.clone();
    second_request.headers.insert(
        "x-mcp-management-session-id".to_string(),
        "mcp-session-b".to_string(),
    );
    second_request.headers.insert(
        "x-mcp-management-run-id".to_string(),
        "task-run-b".to_string(),
    );

    let first_opened = call_builtin_compatible_local_tool(
        &first_request,
        &state,
        "open_edit_session",
        json!({}),
        &recorder,
    )
    .await
    .expect("open first edit session")
    .expect("first open result");
    let first_session_id = code_maintainer_structured_result(first_opened)["result"]["session_id"]
        .as_str()
        .expect("first session id")
        .to_string();
    let second_opened = call_builtin_compatible_local_tool(
        &second_request,
        &state,
        "open_edit_session",
        json!({}),
        &recorder,
    )
    .await
    .expect("open second edit session")
    .expect("second open result");
    let second_session_id = code_maintainer_structured_result(second_opened)["result"]
        ["session_id"]
        .as_str()
        .expect("second session id")
        .to_string();

    assert_ne!(first_session_id, second_session_id);

    for (request, session_id, path, content) in [
        (&first_request, &first_session_id, "first.txt", "first"),
        (&second_request, &second_session_id, "second.txt", "second"),
    ] {
        call_builtin_compatible_local_tool(
            request,
            &state,
            "stage_edit_batch",
            json!({
                "session_id": session_id,
                "operations": [{
                    "kind": "write",
                    "path": path,
                    "content": content,
                    "expected_sha256": null
                }]
            }),
            &recorder,
        )
        .await
        .expect("stage isolated edit session")
        .expect("stage result");
    }

    for (request, session_id) in [
        (&first_request, &first_session_id),
        (&second_request, &second_session_id),
    ] {
        call_builtin_compatible_local_tool(
            request,
            &state,
            "commit_edit_session",
            json!({ "session_id": session_id }),
            &recorder,
        )
        .await
        .expect("commit isolated edit session")
        .expect("commit result");
    }

    assert_eq!(
        fs::read_to_string(project.join("first.txt")).expect("read first file"),
        "first"
    );
    assert_eq!(
        fs::read_to_string(project.join("second.txt")).expect("read second file"),
        "second"
    );
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[tokio::test(flavor = "multi_thread")]
async fn code_maintainer_default_tool_root_scopes_missing_owned_path() {
    let root = temp_test_dir("default-tool-root");
    fs::create_dir_all(root.join("frontend").as_path()).expect("create existing sibling");
    fs::write(root.join("frontend").join("package.json"), "{}\n").expect("write frontend");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_full_control_workspace(workspace);
    let mut request = request_with_cwd_and_builtin_kinds(
        ".",
        "CodeMaintainerRead,CodeMaintainerWrite,TerminalController",
    );
    request.headers.insert(
        "x-local-connector-default-tool-root".to_string(),
        "backend".to_string(),
    );
    let recorder = CommandHistoryRecorder {
        state_path: root.join("state.json"),
        state: Arc::new(RwLock::new(state.clone())),
    };

    let listed = call_builtin_compatible_local_tool(
        &request,
        &state,
        "list_dir",
        json!({ "path": ".", "max_entries": 20 }),
        &recorder,
    )
    .await
    .expect("list call")
    .expect("list result");
    let structured = code_maintainer_structured_result(listed);
    assert_eq!(
        structured.get("entries").and_then(Value::as_array).unwrap(),
        &Vec::<Value>::new()
    );

    let opened = call_builtin_compatible_local_tool(
        &request,
        &state,
        "open_edit_session",
        json!({}),
        &recorder,
    )
    .await
    .expect("open edit session")
    .expect("open result");
    let session_id = code_maintainer_structured_result(opened)["result"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    call_builtin_compatible_local_tool(
        &request,
        &state,
        "stage_edit_batch",
        json!({
            "session_id": session_id.clone(),
            "operations": [{
                "kind": "write",
                "path": "pom.xml",
                "content": "<project />\n",
                "expected_sha256": null
            }]
        }),
        &recorder,
    )
    .await
    .expect("stage edit batch")
    .expect("stage result");
    call_builtin_compatible_local_tool(
        &request,
        &state,
        "commit_edit_session",
        json!({ "session_id": session_id }),
        &recorder,
    )
    .await
    .expect("commit edit session")
    .expect("commit result");

    assert_eq!(
        fs::read_to_string(root.join("backend").join("pom.xml")).expect("read backend pom"),
        "<project />\n"
    );
    assert!(!root.join("pom.xml").exists());

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
    let stdout = structured
        .get("stdout")
        .or_else(|| structured.get("output"))
        .and_then(Value::as_str)
        .unwrap()
        .replace('\\', "/");
    assert!(stdout.contains("backend"));
    assert!(!stdout.contains("frontend"));
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[tokio::test(flavor = "multi_thread")]
async fn full_browser_cdp_tool_tracks_the_device_runtime_setting() {
    let root = temp_test_dir("browser-full-cdp-setting");
    let workspace = test_workspace(root.as_path());
    let mut state = test_state_with_full_control_workspace(workspace);
    let request = request_with_cwd_and_builtin_kinds(".", "BrowserTools");

    let disabled = local_mcp_builtin_compatible_tools(&request, &state).expect("list tools");
    let disabled_names = disabled
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    if !disabled_names.contains("browser_navigate") {
        return;
    }
    assert!(!disabled_names.contains("browser_cdp_command"));

    state.runtime_settings.browser_full_cdp_access_enabled = true;
    let enabled = local_mcp_builtin_compatible_tools(&request, &state).expect("list enabled tools");
    let enabled_names = enabled
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    assert!(enabled_names.contains("browser_cdp_command"));

    state.runtime_settings.browser_full_cdp_access_enabled = false;
    let disabled_again =
        local_mcp_builtin_compatible_tools(&request, &state).expect("list disabled tools again");
    assert!(!disabled_again
        .iter()
        .any(|tool| { tool.get("name").and_then(Value::as_str) == Some("browser_cdp_command") }));
}

#[tokio::test(flavor = "multi_thread")]
async fn without_selected_builtin_kinds_exposes_no_tools() {
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
async fn respects_selected_builtin_kind_header() {
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
    assert!(!names.contains("open_edit_session"));
    assert!(!names.contains("stage_edit_batch"));
    assert!(!names.contains("execute_command"));
    assert!(!names.contains("browser_navigate"));

    request.body = json!({
        "jsonrpc": "2.0",
        "id": "blocked-write",
        "method": "tools/call",
        "params": {
            "name": "stage_edit_batch",
            "arguments": {
                "session_id": "missing-session",
                "operations": [{
                    "kind": "write",
                    "path": "package.json",
                    "content": "{}\n",
                    "expected_sha256": null
                }]
            }
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
async fn local_command_approval_tool_validates_and_returns_structured_decision() {
    let root = temp_test_dir("local-command-approval-tool");
    let project = root.join("apps").join("web");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_workspace(workspace);
    let mut request = request_with_cwd_and_builtin_kinds("apps/web", "LocalCommandApproval");
    let recorder = CommandHistoryRecorder {
        state_path: root.join("state.json"),
        state: Arc::new(RwLock::new(state.clone())),
    };

    let tools = local_mcp_builtin_compatible_tools(&request, &state).expect("list tools");
    assert_eq!(
        tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec!["approval_decision"]
    );

    request.body = json!({
        "jsonrpc": "2.0",
        "id": "decision-1",
        "method": "tools/call",
        "params": {
            "name": "approval_decision",
            "arguments": {
                "decision": "deny",
                "reason": "command escapes the project workspace"
            }
        }
    });
    let response = handle_mcp_body(&request, &state, &recorder)
        .await
        .expect("approval decision response");
    assert_eq!(
        response.pointer("/result/_structured_result/decision"),
        Some(&json!("deny"))
    );
    assert_eq!(
        response.pointer("/result/_structured_result/reason"),
        Some(&json!("command escapes the project workspace"))
    );

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[tokio::test(flavor = "multi_thread")]
async fn lifecycle_starts_and_cleans_task_terminal() {
    let root = temp_test_dir("lifecycle-terminal");
    let project = root.join("apps").join("web");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_full_control_workspace(workspace);
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

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn process_kill_terminates_nested_background_processes() {
    let root = temp_test_dir("terminal-process-tree");
    let project = root.join("apps").join("web");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let state = test_state_with_full_control_workspace(workspace);
    let recorder = CommandHistoryRecorder {
        state_path: root.join("state.json"),
        state: Arc::new(RwLock::new(state.clone())),
    };
    let mut request = request_with_cwd_and_builtin_kinds("apps/web", "TerminalController");
    request.headers.insert(
        "x-task-runner-task-id".to_string(),
        "task-process-tree".to_string(),
    );

    let started = call_builtin_compatible_local_tool(
        &request,
        &state,
        "execute_command",
        json!({
            "path": ".",
            "common": r#"sh -c 'sleep 30 & child=$!; echo "$child"; wait'"#,
            "background": true,
        }),
        &recorder,
    )
    .await
    .expect("start background process tree")
    .expect("background process result");
    let structured = code_maintainer_structured_result(started);
    let terminal_id = structured
        .get("terminal_id")
        .and_then(Value::as_str)
        .expect("background terminal id")
        .to_string();

    let mut child_pid = None;
    for _ in 0..100 {
        let polled = call_builtin_compatible_local_tool(
            &request,
            &state,
            "process_poll",
            json!({ "terminal_id": terminal_id, "limit": 20 }),
            &recorder,
        )
        .await
        .expect("poll background process tree")
        .expect("background process poll result");
        let structured = code_maintainer_structured_result(polled);
        child_pid = structured
            .get("logs")
            .and_then(Value::as_array)
            .and_then(|logs| {
                logs.iter().find_map(|entry| {
                    entry
                        .get("content")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .and_then(|value| value.parse::<i32>().ok())
                })
            });
        if child_pid.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let child_pid = child_pid.expect("nested child pid");
    assert!(unix_process_exists(child_pid));

    let killed = call_builtin_compatible_local_tool(
        &request,
        &state,
        "process_kill",
        json!({ "terminal_id": terminal_id }),
        &recorder,
    )
    .await
    .expect("kill background process tree")
    .expect("process kill result");
    let structured = code_maintainer_structured_result(killed);
    assert_eq!(
        structured.get("killed").and_then(Value::as_bool),
        Some(true)
    );

    for _ in 0..100 {
        if !unix_process_exists(child_pid) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(!unix_process_exists(child_pid));

    let context = local_terminal_controller_context_for_root(
        project.as_path(),
        &request,
        DEFAULT_TERMINAL_EXEC_TIMEOUT_MS,
    );
    LocalConnectorTerminalControllerStore
        .kill_sessions_for_context(context)
        .await
        .expect("cleanup local terminal sessions");
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[cfg(unix)]
fn unix_process_exists(pid: i32) -> bool {
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}
