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
async fn lifecycle_starts_and_cleans_task_terminal() {
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
