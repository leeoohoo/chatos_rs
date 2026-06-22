use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::json;

use super::*;

#[derive(Debug, Clone)]
struct NoopTerminalStore;

#[async_trait]
impl TerminalControllerStore for NoopTerminalStore {
    async fn execute_command(
        &self,
        _context: TerminalControllerContext,
        _path: String,
        command: String,
        _background: bool,
    ) -> Result<Value, String> {
        Ok(json!({ "common": command, "output": "" }))
    }

    async fn get_recent_logs(
        &self,
        _context: TerminalControllerContext,
        _per_terminal_limit: i64,
        _terminal_limit: usize,
    ) -> Result<Value, String> {
        Ok(json!({ "terminals": [] }))
    }

    async fn process_list(
        &self,
        _context: TerminalControllerContext,
        _include_exited: bool,
        _limit: usize,
    ) -> Result<Value, String> {
        Ok(json!({ "processes": [] }))
    }

    async fn process_poll(
        &self,
        _context: TerminalControllerContext,
        terminal_id: String,
        _offset: Option<i64>,
        _limit: i64,
    ) -> Result<Value, String> {
        Ok(json!({ "terminal_id": terminal_id }))
    }

    async fn process_log(
        &self,
        _context: TerminalControllerContext,
        terminal_id: String,
        _offset: Option<i64>,
        _limit: i64,
    ) -> Result<Value, String> {
        Ok(json!({ "terminal_id": terminal_id, "output": "" }))
    }

    async fn process_wait(
        &self,
        _context: TerminalControllerContext,
        terminal_id: String,
        _timeout_ms: u64,
    ) -> Result<Value, String> {
        Ok(json!({ "terminal_id": terminal_id, "wait_status": "completed" }))
    }

    async fn process_write(
        &self,
        _context: TerminalControllerContext,
        terminal_id: String,
        _data: String,
        _submit: bool,
    ) -> Result<Value, String> {
        Ok(json!({ "terminal_id": terminal_id, "operation_status": "ok" }))
    }

    async fn process_kill(
        &self,
        _context: TerminalControllerContext,
        terminal_id: String,
    ) -> Result<Value, String> {
        Ok(json!({ "terminal_id": terminal_id, "operation_status": "killed" }))
    }
}

fn temp_root() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "terminal-controller-tools-{}-{unique}",
        std::process::id()
    ))
}

fn test_service(root: PathBuf) -> TerminalControllerService {
    TerminalControllerService::new(TerminalControllerOptions {
        root,
        user_id: None,
        project_id: None,
        idle_timeout_ms: 1_000,
        max_wait_ms: 5_000,
        max_output_chars: 4_000,
        store: TerminalControllerStoreRef::new(Arc::new(NoopTerminalStore)),
    })
    .expect("create terminal controller")
}

#[test]
fn terminal_controller_registers_process_tools() {
    let root = temp_root();
    std::fs::create_dir_all(&root).expect("create temp root");
    let service = test_service(root.clone());
    let tools = service.list_tools();
    let mut names: Vec<String> = tools
        .iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
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
    let execute_required = execute_schema
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        execute_required
            .iter()
            .all(|value| value.as_str() != Some("path")),
        "execute_command should not require path"
    );
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
    assert!(coerce_process_identifier(Some(&json!("   "))).is_none());
    assert!(coerce_process_identifier(Some(&json!(true))).is_none());
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
        PROCESS_WAIT_MAX_TIMEOUT_MS
    );
}
