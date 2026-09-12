// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, Write},
};

use chatos_mcp_client::{
    LocalMcpExecutor, LocalMcpServerConfig, LocalMcpToolCall, StdioMcpExecutor,
};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

fn main() {
    if std::env::var_os("CHATOS_MCP_FIXTURE_CHILD").is_some() {
        fixture_server();
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(run_contract());
}

async fn run_contract() {
    let executable = std::env::current_exe().expect("test executable");
    let working_directory = executable
        .parent()
        .expect("test executable parent")
        .to_path_buf();
    let executor = StdioMcpExecutor::connect(
        vec![LocalMcpServerConfig {
            name: "plugin-fixture".to_string(),
            executable,
            arguments: Vec::new(),
            working_directory,
            environment: BTreeMap::from([(
                "CHATOS_MCP_FIXTURE_CHILD".to_string(),
                "1".to_string(),
            )]),
        }],
        BTreeSet::from(["plugin-fixture_render".to_string()]),
    )
    .await
    .expect("initialized stdio MCP executor");

    let tools = executor.available_tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "plugin-fixture_render");
    assert_eq!(tools[0]["parameters"]["additionalProperties"], false);

    let result = executor
        .execute_tool(
            LocalMcpToolCall {
                tool_call_id: "call-1".to_string(),
                tool_name: "plugin-fixture_render".to_string(),
                arguments: json!({"title": "Landing"}),
                run_id: "run-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
            CancellationToken::new(),
        )
        .await
        .expect("tool result");
    assert_eq!(result.content, "rendered Landing");
    assert_eq!(
        result.structured_result,
        Some(json!({"frame_id": "frame-1", "title": "Landing"}))
    );
    assert!(!result.is_error);
}

fn fixture_server() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.expect("stdin line");
        let request: Value = serde_json::from_str(&line).expect("JSON-RPC request");
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let result = match request.get("method").and_then(Value::as_str) {
            Some("initialize") => json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "fixture", "version": "1.0.0"}
            }),
            Some("tools/list") => json!({
                "tools": [{
                    "name": "render",
                    "description": "Render one design frame",
                    "inputSchema": {
                        "type": "object",
                        "properties": {"title": {"type": "string"}},
                        "required": ["title"]
                    }
                }]
            }),
            Some("tools/call") => {
                let title = request["params"]["arguments"]["title"]
                    .as_str()
                    .unwrap_or_default();
                assert_eq!(request["params"]["_meta"]["chatos/runId"], "run-1");
                assert_eq!(request["params"]["_meta"]["chatos/turnId"], "turn-1");
                json!({
                    "content": [{"type": "text", "text": format!("rendered {title}")}],
                    "structuredContent": {"frame_id": "frame-1", "title": title},
                    "isError": false
                })
            }
            other => panic!("unexpected method: {other:?}"),
        };
        writeln!(
            stdout,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        )
        .expect("stdout response");
        stdout.flush().expect("stdout flush");
    }
}
