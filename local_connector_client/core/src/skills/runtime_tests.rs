// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;

use chatos_mcp_runtime::{BuiltinToolProvider, ToolCallContext};
use chatos_plugin_management_sdk::{
    AgentBindingRecord, BindingConditions, ResolvedSkill, ResourceMetadata, SkillContent,
    SkillInstallationRecord, SkillRecord,
};
use serde_json::json;
use uuid::Uuid;

use crate::relay::RelayRequest;
use crate::skills::local_skill_inventory;
use crate::LocalState;

use super::prepare_local_skill;

#[tokio::test]
async fn prepares_and_executes_selected_skill_on_the_client() {
    let root = std::env::temp_dir().join(format!("chatos-local-skill-{}", Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("create local Skill workspace");
    let inventory = local_skill_inventory()
        .expect("local Skill inventory")
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_visualize")
        .expect("visualize Skill inventory");
    let state = serde_json::from_value::<LocalState>(json!({
        "device_id": "device-1",
        "workspaces": [{
            "id": "workspace-1",
            "absolute_root": root.to_string_lossy(),
            "alias": "skill-workspace",
            "fingerprint": "skill-workspace"
        }]
    }))
    .expect("local Skill state");
    let request = RelayRequest {
        _message_type: "local_runtime_chat".to_string(),
        request_id: "turn-1".to_string(),
        owner_user_id: Some("user-1".to_string()),
        device_id: Some("device-1".to_string()),
        workspace_id: "workspace-1".to_string(),
        method: None,
        path: None,
        headers: BTreeMap::new(),
        body: json!({}),
    };
    let prepared = prepare_local_skill(&resolved_skill(&inventory), &state, &request)
        .expect("prepare selected local Skill");
    let server = prepared.server.expect("Skill server");
    let provider = prepared.provider.expect("Skill provider");
    assert_eq!(server.name, "local_skill_visualize");
    assert!(provider.list_tools().iter().any(|tool| {
        tool.get("name").and_then(serde_json::Value::as_str) == Some("write_visualization_html")
    }));

    provider
        .call_tool(
            "write_visualization_html",
            json!({
                "target_path": "artifacts/demo.html",
                "title": "Demo",
                "body_html": "<main>local</main>"
            }),
            ToolCallContext::new(None, None, None),
            None,
        )
        .await
        .expect("execute selected local Skill tool");
    assert!(root.join("artifacts/demo.html").exists());

    fs::remove_dir_all(root).expect("cleanup local Skill workspace");
}

fn resolved_skill(inventory: &crate::skills::LocalSkillInventoryItem) -> ResolvedSkill {
    ResolvedSkill {
        resource: SkillRecord {
            id: inventory.skill_id.clone(),
            owner_user_id: "system".to_string(),
            owner_kind: "system".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "system_seed".to_string(),
            name: "visualize".to_string(),
            display_name: "Visualize".to_string(),
            description: None,
            enabled: true,
            content: SkillContent {
                kind: "local_connector_bundle".to_string(),
                bundle_id: Some(inventory.bundle_id.clone()),
                bundle_version: Some(inventory.version.clone()),
                bundle_hash: Some(inventory.bundle_hash.clone()),
                ..SkillContent::default()
            },
            metadata: ResourceMetadata::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        binding: AgentBindingRecord {
            id: "binding-visualize".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            binding_scope: "global_default".to_string(),
            owner_user_id: None,
            resource_kind: "skill".to_string(),
            resource_id: inventory.skill_id.clone(),
            enabled: true,
            required: false,
            priority: 0,
            conditions: BindingConditions::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available: true,
        status: "available".to_string(),
        reason: None,
        installation: Some(SkillInstallationRecord {
            id: "installation-visualize".to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            skill_id: inventory.skill_id.clone(),
            bundle_id: inventory.bundle_id.clone(),
            version: inventory.version.clone(),
            bundle_hash: inventory.bundle_hash.clone(),
            platform: "test".to_string(),
            status: "available".to_string(),
            dependency_status: "available".to_string(),
            last_error: None,
            last_checked_at: "now".to_string(),
        }),
    }
}
