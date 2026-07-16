// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;

use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::local_runtime::LocalDatabase;
use crate::mcp::manifest::{LocalMcpHttpConfig, LocalMcpManifestRecord, LocalMcpTransport};
use crate::relay::RelayRequest;

use super::handle_user_mcp_body;

#[tokio::test]
async fn executes_user_mcp_from_sqlite_manifest() {
    let app = Router::new().route(
        "/mcp",
        post(|Json(request): Json<Value>| async move {
            Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "result": {"tools": [{
                    "name": "demo_tool",
                    "description": "demo",
                    "inputSchema": {"type": "object"}
                }]}
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind user MCP server");
    let url = format!(
        "http://{}/mcp",
        listener.local_addr().expect("user MCP address")
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let root = std::env::temp_dir().join(format!("chatos-user-mcp-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open user MCP database");
    database
        .save_mcp_manifest(&manifest(url))
        .await
        .expect("save user MCP manifest");
    let request = RelayRequest {
        _message_type: "mcp_request".to_string(),
        request_id: "request-1".to_string(),
        owner_user_id: Some("owner-1".to_string()),
        device_id: Some("device-1".to_string()),
        workspace_id: "workspace-1".to_string(),
        method: None,
        path: None,
        headers: BTreeMap::from([
            (
                "x-local-connector-mcp-manifest-id".to_string(),
                "manifest-1".to_string(),
            ),
            (
                "x-plugin-management-resource-id".to_string(),
                "plugin-1".to_string(),
            ),
        ]),
        body: json!({"jsonrpc":"2.0","id":"rpc-1","method":"tools/list"}),
    };
    let response = handle_user_mcp_body(&request, &database)
        .await
        .expect("execute SQLite user MCP");
    assert_eq!(
        response
            .pointer("/result/tools/0/name")
            .and_then(Value::as_str),
        Some("demo_tool")
    );

    server.abort();
    database.close().await;
    fs::remove_dir_all(root).expect("cleanup user MCP database");
}

fn manifest(url: String) -> LocalMcpManifestRecord {
    LocalMcpManifestRecord {
        manifest_id: "manifest-1".to_string(),
        plugin_mcp_id: Some("plugin-1".to_string()),
        owner_user_id: "owner-1".to_string(),
        device_id: "device-1".to_string(),
        internal_name: "user_mcp_manifest1".to_string(),
        display_name: "User MCP".to_string(),
        description: None,
        transport: LocalMcpTransport::Http,
        stdio: None,
        http: Some(LocalMcpHttpConfig {
            url,
            headers: BTreeMap::new(),
            timeout_ms: 5_000,
        }),
        enabled: true,
        sync_status: "synced".to_string(),
        last_check_status: "available".to_string(),
        last_checked_at: Some("now".to_string()),
        last_error: None,
        tool_snapshot: Vec::new(),
        manifest_hash: "hash-1".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}
