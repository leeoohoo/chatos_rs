// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use chatos_plugin_management_sdk::ResolvedAgentCapabilities;
use uuid::Uuid;

use super::LocalDatabase;

#[tokio::test]
async fn replaces_capability_snapshot_without_deleting_last_valid_payload() {
    let root = std::env::temp_dir().join(format!("chatos-capabilities-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open capability database");
    let mut snapshot = capabilities("revision-1");
    database
        .save_capability_snapshot(&snapshot)
        .await
        .expect("save first snapshot");
    snapshot.policy_revision = "revision-2".to_string();
    database
        .save_capability_snapshot(&snapshot)
        .await
        .expect("replace snapshot");

    let stored = database
        .get_capability_snapshot("user-1", "chatos_conversation_agent")
        .await
        .expect("load snapshot")
        .expect("snapshot exists");
    assert_eq!(stored.policy_revision, "revision-2");

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup capability database");
}

fn capabilities(revision: &str) -> ResolvedAgentCapabilities {
    ResolvedAgentCapabilities {
        agent_key: "chatos_conversation_agent".to_string(),
        owner_user_id: "user-1".to_string(),
        policy_revision: revision.to_string(),
        generated_at: "2026-07-15T00:00:00Z".to_string(),
        agent_enabled: true,
        mcps: Vec::new(),
        skills: Vec::new(),
        local_connector_requirements: Vec::new(),
    }
}
