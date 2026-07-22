// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use chatos_plugin_management_sdk::{
    agent_prompt_checksum, AgentPromptBundle, AgentPromptBundleManifest, AgentPromptVendor,
    ResolvedAgentCapabilities, ResolvedAgentPrompt, SystemAgentKey,
};
use uuid::Uuid;

use crate::local_runtime::storage::LocalDatabase;

fn complete_bundle(version: i64) -> AgentPromptBundle {
    let prompts = SystemAgentKey::ALL
        .into_iter()
        .flat_map(|agent| {
            AgentPromptVendor::ALL.into_iter().map(move |vendor| {
                let content = format!("{} {vendor} prompt", agent.as_str());
                ResolvedAgentPrompt {
                    agent_key: agent.as_str().to_string(),
                    vendor,
                    checksum: agent_prompt_checksum(content.as_str()),
                    content,
                    revision: version,
                    published_at: "2026-07-16T00:00:00Z".to_string(),
                }
            })
        })
        .collect();
    AgentPromptBundle {
        bundle_version: version,
        updated_at: "2026-07-16T00:00:00Z".to_string(),
        prompts,
    }
}

fn complete_capability_bundle(
    owner_user_id: &str,
    revision: &str,
) -> Vec<ResolvedAgentCapabilities> {
    SystemAgentKey::ALL
        .into_iter()
        .map(|agent_key| ResolvedAgentCapabilities {
            agent_key: agent_key.as_str().to_string(),
            owner_user_id: owner_user_id.to_string(),
            policy_revision: format!("{revision}:{}", agent_key.as_str()),
            generated_at: "2026-07-16T00:00:00Z".to_string(),
            agent_enabled: true,
            mcps: Vec::new(),
            skills: Vec::new(),
            local_connector_requirements: Vec::new(),
        })
        .collect()
}

#[tokio::test]
async fn installs_bundle_atomically_and_tracks_remote_updates() {
    let root = std::env::temp_dir().join(format!("chatos-agent-prompts-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open database");
    let source = "https://cloud.example.test";

    let capabilities = complete_capability_bundle("user-1", "policy-3");
    database
        .install_agent_configuration_bundle(source, &complete_bundle(3), &capabilities)
        .await
        .expect("install complete Agent configuration bundle");
    let prompt = database
        .get_installed_agent_prompt(
            source,
            SystemAgentKey::ChatosConversationAgent,
            AgentPromptVendor::Gpt,
        )
        .await
        .expect("load prompt")
        .expect("installed prompt");
    assert_eq!(prompt.bundle_version, 3);

    database
        .save_agent_prompt_manifest(
            source,
            &AgentPromptBundleManifest {
                bundle_version: 4,
                updated_at: "2026-07-16T01:00:00Z".to_string(),
                required: false,
            },
            false,
        )
        .await
        .expect("save manifest");
    let sync = database
        .get_agent_prompt_sync_state()
        .await
        .expect("load sync")
        .expect("sync state");
    assert_eq!(sync.installed_bundle_version, 3);
    assert_eq!(sync.remote_bundle_version, 4);
    assert!(sync.update_available);
    assert_eq!(
        sync.prompt_count,
        (SystemAgentKey::ALL.len() * AgentPromptVendor::ALL.len()) as i64
    );
    assert_eq!(
        database
            .count_capability_snapshots("user-1")
            .await
            .expect("count capability snapshots"),
        SystemAgentKey::ALL.len() as i64
    );

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup database");
}

#[tokio::test]
async fn rejects_incomplete_configuration_bundle_without_replacing_last_valid_installation() {
    let root = std::env::temp_dir().join(format!("chatos-agent-config-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open database");
    let source = "https://cloud.example.test";
    let installed = complete_capability_bundle("user-1", "policy-1");
    database
        .install_agent_configuration_bundle(source, &complete_bundle(1), &installed)
        .await
        .expect("install initial complete configuration");

    let mut incomplete = complete_capability_bundle("user-1", "policy-2");
    incomplete.pop();
    database
        .install_agent_configuration_bundle(source, &complete_bundle(2), &incomplete)
        .await
        .expect_err("incomplete Plugin configuration must fail before transaction");

    let prompt = database
        .get_installed_agent_prompt(
            source,
            SystemAgentKey::ChatosConversationAgent,
            AgentPromptVendor::Gpt,
        )
        .await
        .expect("load retained prompt")
        .expect("retained prompt exists");
    assert_eq!(prompt.bundle_version, 1);
    let retained = database
        .get_capability_snapshot("user-1", SystemAgentKey::ChatosConversationAgent.as_str())
        .await
        .expect("load retained capability")
        .expect("retained capability exists");
    assert!(retained.policy_revision.starts_with("policy-1:"));

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup database");
}
