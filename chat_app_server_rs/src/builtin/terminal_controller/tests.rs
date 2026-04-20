use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::context::{build_input_payload, normalize_shell_input};
use super::{
    coerce_process_identifier, resolve_wait_timeout_ms, TerminalControllerOptions,
    TerminalControllerService,
};

#[test]
fn normalize_shell_input_uses_windows_return_key() {
    let raw = "echo one\necho two\r\necho three\r";
    let normalized = normalize_shell_input(raw);

    if cfg!(windows) {
        assert_eq!(normalized, "echo one\recho two\recho three\r");
    } else {
        assert_eq!(normalized, raw);
    }
}

#[test]
fn build_input_payload_uses_shell_line_endings() {
    let root = if cfg!(windows) {
        Path::new(r"C:\repo\sandbox")
    } else {
        Path::new("/tmp/repo/sandbox")
    };

    let payload = build_input_payload(root, root, "echo hi");

    if cfg!(windows) {
        assert!(payload.contains("\r"));
        assert!(!payload.contains("\n"));
    } else {
        assert!(payload.contains("\n"));
    }
}

#[test]
fn terminal_controller_registers_process_tools() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "terminal-controller-tools-{}-{}",
        std::process::id(),
        unique
    ));
    std::fs::create_dir_all(&root).expect("create temp root");

    let service = TerminalControllerService::new(TerminalControllerOptions {
        root: root.clone(),
        user_id: None,
        project_id: None,
        idle_timeout_ms: 1_000,
        max_wait_ms: 5_000,
        max_output_chars: 4_000,
    })
    .expect("create terminal controller");

    let tools = service.list_tools();
    let mut names: Vec<String> = tools
        .iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(|v| v.to_string())
        })
        .collect();
    names.sort();

    for expected in [
        "execute_command",
        "get_recent_logs",
        "process",
        "process_kill",
        "process_list",
        "process_log",
        "process_poll",
        "process_wait",
        "process_write",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing tool: {expected}"
        );
    }

    let poll_schema = tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("process_poll"))
        .and_then(|tool| tool.get("inputSchema"))
        .expect("process_poll schema");
    let required = poll_schema
        .get("required")
        .and_then(Value::as_array)
        .expect("process_poll required");
    assert!(
        required
            .iter()
            .any(|value| value.as_str() == Some("terminal_id")),
        "process_poll must require terminal_id"
    );

    let execute_schema = tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("execute_command"))
        .and_then(|tool| tool.get("inputSchema"))
        .expect("execute_command schema");
    let has_background = execute_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| props.contains_key("background"))
        .unwrap_or(false);
    assert!(
        has_background,
        "execute_command should expose background switch"
    );

    let process_schema = tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("process"))
        .and_then(|tool| tool.get("inputSchema"))
        .expect("process schema");
    let process_required = process_schema
        .get("required")
        .and_then(Value::as_array)
        .expect("process required");
    assert!(
        process_required
            .iter()
            .any(|value| value.as_str() == Some("action")),
        "process must require action"
    );
    let process_actions = process_schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|props| props.get("action"))
        .and_then(Value::as_object)
        .and_then(|item| item.get("enum"))
        .and_then(Value::as_array)
        .expect("process action enum");
    assert!(
        process_actions
            .iter()
            .any(|value| value.as_str() == Some("log")),
        "process(action) should include log for Hermes compatibility"
    );
    let has_timeout_alias = process_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| props.contains_key("timeout"))
        .unwrap_or(false);
    assert!(
        has_timeout_alias,
        "process schema should expose timeout alias (seconds)"
    );

    let process_wait_schema = tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("process_wait"))
        .and_then(|tool| tool.get("inputSchema"))
        .expect("process_wait schema");
    let process_wait_has_timeout_alias = process_wait_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| props.contains_key("timeout"))
        .unwrap_or(false);
    assert!(
        process_wait_has_timeout_alias,
        "process_wait schema should expose timeout alias (seconds)"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn coerce_process_identifier_supports_numeric_value() {
    assert_eq!(
        coerce_process_identifier(Some(&json!(123456))),
        Some("123456".to_string())
    );
    assert_eq!(
        coerce_process_identifier(Some(&json!("  abc-123  "))),
        Some("abc-123".to_string())
    );
    assert!(
        coerce_process_identifier(Some(&json!("   "))).is_none(),
        "blank string should not be treated as a valid process identifier"
    );
    assert!(
        coerce_process_identifier(Some(&json!(true))).is_none(),
        "non-string/non-number identifier should be rejected"
    );
}

#[test]
fn resolve_wait_timeout_ms_supports_timeout_alias_seconds() {
    assert_eq!(resolve_wait_timeout_ms(&json!({})), 30_000);
    assert_eq!(resolve_wait_timeout_ms(&json!({ "timeout": 7 })), 7_000);
    assert_eq!(
        resolve_wait_timeout_ms(&json!({ "timeout_ms": 2_500, "timeout": 7 })),
        2_500
    );
    assert_eq!(
        resolve_wait_timeout_ms(&json!({ "timeout": 999_999 })),
        super::PROCESS_WAIT_MAX_TIMEOUT_MS
    );
}
