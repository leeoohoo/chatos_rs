// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::*;

#[derive(Debug, Clone)]
struct NoopRemoteStore;

#[async_trait]
impl RemoteConnectionControllerStore for NoopRemoteStore {
    async fn list_connections(
        &self,
        _context: RemoteConnectionControllerContext,
    ) -> Result<Value, String> {
        Ok(json!({ "count": 0, "connections": [] }))
    }

    async fn test_connection(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
    ) -> Result<Value, String> {
        Ok(json!({ "result": "ok" }))
    }

    async fn run_command(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        command: String,
        _timeout_seconds: Option<u64>,
        _allow_dangerous: bool,
        _max_output_chars: Option<usize>,
    ) -> Result<Value, String> {
        Ok(json!({ "command": command, "output": "" }))
    }

    async fn list_directory(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        _path: Option<String>,
        _limit: Option<usize>,
    ) -> Result<Value, String> {
        Ok(json!({ "entries": [] }))
    }

    async fn read_file(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        path: String,
        _max_bytes: Option<usize>,
    ) -> Result<Value, String> {
        Ok(json!({ "path": path, "content": "" }))
    }

    async fn download_file(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        path: String,
        encoding: String,
        _max_bytes: Option<usize>,
    ) -> Result<Value, String> {
        Ok(json!({ "path": path, "encoding": encoding, "content": "" }))
    }

    async fn upload_file(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        path: String,
        content: String,
        encoding: String,
        create_parent_dirs: bool,
        overwrite: bool,
    ) -> Result<Value, String> {
        Ok(json!({
            "path": path,
            "encoding": encoding,
            "bytes_written": content.len(),
            "create_parent_dirs": create_parent_dirs,
            "overwrite": overwrite,
        }))
    }
}

fn option_base() -> RemoteConnectionControllerOptions {
    RemoteConnectionControllerOptions {
        server_name: "remote_connection_controller".to_string(),
        user_id: Some("u1".to_string()),
        default_remote_connection_id: None,
        command_timeout_seconds: 20,
        max_command_timeout_seconds: 120,
        max_output_chars: 20_000,
        max_read_file_bytes: 256 * 1024,
        store: RemoteConnectionControllerStoreRef::new(Arc::new(NoopRemoteStore)),
    }
}

fn find_required_for_tool(tools: &[Value], name: &str) -> Vec<String> {
    tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|tool| tool.get("inputSchema"))
        .and_then(|schema| schema.get("required"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[test]
fn hides_tools_when_user_context_is_missing() {
    let mut options = option_base();
    options.user_id = None;
    let service = RemoteConnectionControllerService::new(options).expect("init");
    assert!(service.list_tools().is_empty());
    let unavailable = service.unavailable_tools();
    assert_eq!(unavailable.len(), 7);
    for name in [
        "list_connections",
        "test_connection",
        "run_command",
        "list_directory",
        "read_file",
        "download_file",
        "upload_file",
    ] {
        assert!(
            unavailable.iter().any(|(tool_name, _)| tool_name == name),
            "missing unavailable tool: {name}"
        );
    }
}

#[test]
fn requires_connection_id_when_default_connection_is_missing() {
    let service = RemoteConnectionControllerService::new(option_base()).expect("init");
    let tools = service.list_tools();
    assert!(find_required_for_tool(&tools, "test_connection")
        .iter()
        .any(|value| value == "connection_id"));
    let run_required = find_required_for_tool(&tools, "run_command");
    assert!(run_required.iter().any(|value| value == "connection_id"));
    assert!(run_required.iter().any(|value| value == "command"));
    assert!(find_required_for_tool(&tools, "list_directory")
        .iter()
        .any(|value| value == "connection_id"));
    let read_required = find_required_for_tool(&tools, "read_file");
    assert!(read_required.iter().any(|value| value == "connection_id"));
    assert!(read_required.iter().any(|value| value == "path"));
    let download_required = find_required_for_tool(&tools, "download_file");
    assert!(download_required
        .iter()
        .any(|value| value == "connection_id"));
    assert!(download_required.iter().any(|value| value == "path"));
    let upload_required = find_required_for_tool(&tools, "upload_file");
    assert!(upload_required.iter().any(|value| value == "connection_id"));
    assert!(upload_required.iter().any(|value| value == "path"));
    assert!(upload_required.iter().any(|value| value == "content"));
}

#[test]
fn keeps_connection_id_optional_when_default_connection_exists() {
    let mut options = option_base();
    options.default_remote_connection_id = Some("conn_default".to_string());
    let service = RemoteConnectionControllerService::new(options).expect("init");
    let tools = service.list_tools();
    assert!(!find_required_for_tool(&tools, "test_connection")
        .iter()
        .any(|value| value == "connection_id"));
    let run_required = find_required_for_tool(&tools, "run_command");
    assert!(run_required.iter().any(|value| value == "command"));
    assert!(!run_required.iter().any(|value| value == "connection_id"));
    assert!(!find_required_for_tool(&tools, "list_directory")
        .iter()
        .any(|value| value == "connection_id"));
    let read_required = find_required_for_tool(&tools, "read_file");
    assert!(read_required.iter().any(|value| value == "path"));
    assert!(!read_required.iter().any(|value| value == "connection_id"));
    let download_required = find_required_for_tool(&tools, "download_file");
    assert!(download_required.iter().any(|value| value == "path"));
    assert!(!download_required
        .iter()
        .any(|value| value == "connection_id"));
    let upload_required = find_required_for_tool(&tools, "upload_file");
    assert!(upload_required.iter().any(|value| value == "path"));
    assert!(upload_required.iter().any(|value| value == "content"));
    assert!(!upload_required.iter().any(|value| value == "connection_id"));
}

#[test]
fn upload_and_download_tools_validate_encoding() {
    let service = RemoteConnectionControllerService::new(option_base()).expect("init");

    let download = service
        .call_tool(
            "download_file",
            json!({
                "connection_id": "conn",
                "path": "/tmp/a.bin",
                "encoding": "base64",
            }),
        )
        .expect("download");
    assert!(download.to_string().contains("base64"));

    let upload = service
        .call_tool(
            "upload_file",
            json!({
                "connection_id": "conn",
                "path": "/tmp/a.txt",
                "content": "hello",
                "create_parent_dirs": false,
                "overwrite": false,
            }),
        )
        .expect("upload");
    assert!(upload
        .to_string()
        .contains("hello".len().to_string().as_str()));

    let err = service
        .call_tool(
            "download_file",
            json!({
                "connection_id": "conn",
                "path": "/tmp/a.bin",
                "encoding": "hex",
            }),
        )
        .expect_err("invalid encoding");
    assert!(err.contains("encoding must be one of"));
}
