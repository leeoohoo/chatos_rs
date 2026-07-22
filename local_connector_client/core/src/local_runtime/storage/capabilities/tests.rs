// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, SystemAgentKey};
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

fn complete_capabilities(revision: &str) -> Vec<ResolvedAgentCapabilities> {
    SystemAgentKey::ALL
        .into_iter()
        .map(|agent_key| ResolvedAgentCapabilities {
            agent_key: agent_key.as_str().to_string(),
            owner_user_id: "user-1".to_string(),
            policy_revision: format!("{revision}:{}", agent_key.as_str()),
            generated_at: "2026-07-15T00:00:00Z".to_string(),
            agent_enabled: true,
            mcps: Vec::new(),
            skills: Vec::new(),
            local_connector_requirements: Vec::new(),
        })
        .collect()
}

#[tokio::test]
async fn incomplete_snapshot_batch_cannot_delete_the_last_complete_plugin_configuration() {
    let root = std::env::temp_dir().join(format!("chatos-capability-batch-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open capability database");
    let first = complete_capabilities("revision-1");
    database
        .replace_capability_snapshots(&first)
        .await
        .expect("install complete capability batch");

    let mut incomplete = complete_capabilities("revision-2");
    incomplete.pop();
    database
        .replace_capability_snapshots(&incomplete)
        .await
        .expect_err("incomplete capability batch must fail closed");

    assert_eq!(
        database
            .count_capability_snapshots("user-1")
            .await
            .expect("count retained snapshots"),
        SystemAgentKey::ALL.len() as i64
    );
    assert!(database
        .capability_snapshots_match("user-1", &first)
        .await
        .expect("compare retained snapshots"));

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup capability database");
}
