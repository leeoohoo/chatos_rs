// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use uuid::Uuid;

use super::super::{
    CreateLocalSessionInput, LocalDatabase, SaveLocalRuntimeSettingsInput, UpsertLocalProjectInput,
};

#[tokio::test]
async fn persists_per_session_local_memory_policy() {
    let root = std::env::temp_dir().join(format!("chatos-memory-policy-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-policy".to_string(),
            owner_user_id: "user-policy".to_string(),
            device_id: "device-policy".to_string(),
            workspace_id: "workspace-policy".to_string(),
            project_name: "Policy project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-policy".to_string(),
            owner_user_id: "user-policy".to_string(),
            title: "Policy session".to_string(),
            selected_model_id: Some("model-policy".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    let defaults = database
        .get_runtime_settings("user-policy", session.id.as_str())
        .await
        .expect("load default settings")
        .expect("default settings");
    assert!(defaults.memory_auto_summary_enabled);
    assert_eq!(defaults.memory_summary_message_threshold, 24);
    assert_eq!(defaults.memory_summary_character_threshold, 32_000);
    assert_eq!(defaults.memory_recall_limit, 8);

    let saved = database
        .save_runtime_settings(
            "user-policy",
            SaveLocalRuntimeSettingsInput {
                session_id: session.id.clone(),
                selected_model_id: defaults.selected_model_id,
                selected_model_name: defaults.selected_model_name,
                selected_thinking_level: defaults.selected_thinking_level,
                workspace_root: defaults.workspace_root,
                reasoning_enabled: defaults.reasoning_enabled,
                plan_mode_enabled: defaults.plan_mode_enabled,
                mcp_enabled: defaults.mcp_enabled,
                enabled_mcp_ids_json: defaults.enabled_mcp_ids_json,
                selected_skill_ids_json: defaults.selected_skill_ids_json,
                auto_create_task: defaults.auto_create_task,
                memory_auto_summary_enabled: false,
                memory_summary_message_threshold: 40,
                memory_summary_character_threshold: 64_000,
                memory_recall_limit: 12,
            },
        )
        .await
        .expect("save memory policy");
    assert!(!saved.memory_auto_summary_enabled);
    assert_eq!(saved.memory_summary_message_threshold, 40);
    assert_eq!(saved.memory_summary_character_threshold, 64_000);
    assert_eq!(saved.memory_recall_limit, 12);

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup policy database");
}
