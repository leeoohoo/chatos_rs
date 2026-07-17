// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use uuid::Uuid;

use crate::mcp::manifest::{LocalMcpManifestRecord, LocalMcpStdioConfig, LocalMcpTransport};

use super::LocalDatabase;

#[tokio::test]
async fn persists_local_mcp_manifest_only_in_sqlite() {
    let root = std::env::temp_dir().join(format!("chatos-mcp-sqlite-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open MCP database");
    let record = manifest();
    database
        .save_mcp_manifest(&record)
        .await
        .expect("save MCP manifest");
    let stored = database
        .get_mcp_manifest("owner-1", "device-1", "manifest-1")
        .await
        .expect("get MCP manifest")
        .expect("MCP manifest exists");
    assert_eq!(stored.stdio.expect("stdio config").command, "demo-mcp");
    assert!(database
        .delete_mcp_manifest("owner-1", "device-1", "manifest-1")
        .await
        .expect("delete MCP manifest"));

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup MCP database");
}

fn manifest() -> LocalMcpManifestRecord {
    LocalMcpManifestRecord {
        manifest_id: "manifest-1".to_string(),
        plugin_mcp_id: None,
        owner_user_id: "owner-1".to_string(),
        device_id: "device-1".to_string(),
        internal_name: "user_mcp_manifest1".to_string(),
        display_name: "Demo MCP".to_string(),
        description: None,
        transport: LocalMcpTransport::Stdio,
        stdio: Some(LocalMcpStdioConfig {
            command: "demo-mcp".to_string(),
            ..LocalMcpStdioConfig::default()
        }),
        http: None,
        enabled: true,
        sync_status: "pending".to_string(),
        last_check_status: "unknown".to_string(),
        last_checked_at: None,
        last_error: None,
        tool_snapshot: Vec::new(),
        manifest_hash: "hash".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}
