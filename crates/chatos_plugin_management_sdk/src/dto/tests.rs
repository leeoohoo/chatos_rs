// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn agent_tool_plane_defaults_to_managed_and_serializes_stably() {
    assert_eq!(AgentToolPlane::default(), AgentToolPlane::Managed);
    assert!(AgentToolPlane::Managed.supports_tools());
    assert!(AgentToolPlane::Managed.uses_managed_gateway());
    assert!(AgentToolPlane::LocalOnly.supports_tools());
    assert!(!AgentToolPlane::LocalOnly.uses_managed_gateway());
    assert!(!AgentToolPlane::None.supports_tools());
    assert!(!AgentToolPlane::None.uses_managed_gateway());
    assert_eq!(
        serde_json::to_value(AgentToolPlane::LocalOnly).expect("local tool plane JSON"),
        serde_json::json!("local_only")
    );
    assert_eq!(
        serde_json::to_value(AgentToolPlane::None).expect("tool plane JSON"),
        serde_json::json!("none")
    );
}

#[test]
fn system_agent_keys_match_registry_keys() {
    assert_eq!(SystemAgentKey::ALL.len(), 17);
    assert_eq!(SystemAgentKey::ALL.len() * AgentPromptVendor::ALL.len(), 68);
    assert_eq!(
        SystemAgentKey::ChatosConversationAgent.as_str(),
        "chatos_conversation_agent"
    );
    assert_eq!(
        SystemAgentKey::ChatosLocalConversationAgent.as_str(),
        "chatos_local_conversation_agent"
    );
    assert_eq!(
        SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str(),
        "local_connector_command_approval_agent"
    );
    assert_eq!(
        SystemAgentKey::MemoryEngineThreadRepairAgent.as_str(),
        "memory_engine_thread_repair_agent"
    );
    assert!(SystemAgentKey::ALL.contains(&SystemAgentKey::TaskRunnerLocalPlanPhase));
    assert!(SystemAgentKey::ALL.contains(&SystemAgentKey::TaskRunnerLocalRunPhase));
    assert!(SystemAgentKey::ALL.contains(&SystemAgentKey::ProjectManagementLocalAgent));
}

#[test]
fn system_mcp_keys_are_stable_and_complete() {
    assert_eq!(SystemMcpKey::ALL.len(), 19);
    assert!("task_manager".parse::<SystemMcpKey>().is_err());
    assert_eq!(
        SystemMcpKey::ProjectRuntimeEnvironment.as_str(),
        "project_runtime_environment"
    );
    assert_eq!(SystemMcpKey::TaskProcessLog.as_str(), "task_process_log");
    assert_eq!(
        "task_runner_service".parse::<SystemMcpKey>(),
        Ok(SystemMcpKey::TaskRunnerService)
    );
    assert_eq!(
        "task_process_log".parse::<SystemMcpKey>(),
        Ok(SystemMcpKey::TaskProcessLog)
    );
}

#[test]
fn resource_security_default_snapshot_matches_service_policy() {
    let snapshot = serde_json::to_value(ResourceSecurity::default()).expect("security JSON");
    assert_eq!(
        snapshot,
        serde_json::json!({
            "allow_writes": null,
            "max_file_bytes": 262144,
            "max_write_bytes": 5242880,
            "search_limit": 40,
            "allowed_tool_names": [],
            "blocked_tool_names": []
        })
    );
}

#[test]
fn local_connector_status_batch_round_trips_flattened_contract() {
    let snapshot = serde_json::json!({
        "items": [{
            "mcp_id": "mcp-1",
            "owner_user_id": "user-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "manifest_id": "manifest-1",
            "status": "available",
            "last_error": null,
            "tool_snapshot": [{"name": "read_file"}],
            "manifest_hash": "sha256:demo"
        }]
    });

    let batch: LocalConnectorMcpStatusBatchRequest =
        serde_json::from_value(snapshot.clone()).expect("decode status batch");
    assert_eq!(
        batch.items[0].status.workspace_id.as_deref(),
        Some("workspace-1")
    );
    assert_eq!(
        serde_json::to_value(batch).expect("encode status batch"),
        snapshot
    );
}

#[test]
fn plugin_component_ownership_is_flattened_and_legacy_compatible() {
    let mut snapshot = serde_json::json!({
        "id": "mcp-1",
        "owner_user_id": "system",
        "owner_kind": "system",
        "visibility": "system_private",
        "source_kind": "plugin_release",
        "name": "demo",
        "display_name": "Demo",
        "description": null,
        "enabled": true,
        "runtime": {"kind": "http"},
        "security": {},
        "metadata": {},
        "plugin_id": "plugin-1",
        "release_id": "release-1",
        "component_key": "main",
        "managed_by_plugin": true,
        "immutable_from_release": true,
        "created_by": "system",
        "updated_by": "system",
        "created_at": "now",
        "updated_at": "now"
    });
    let record: McpRecord =
        serde_json::from_value(snapshot.clone()).expect("decode Plugin-owned MCP");
    assert_eq!(
        record.plugin_component.complete_identity(),
        Some(("plugin-1", "release-1", "main"))
    );
    let encoded = serde_json::to_value(&record).expect("encode Plugin-owned MCP");
    assert_eq!(encoded["plugin_id"], "plugin-1");
    assert!(encoded.get("plugin_component").is_none());

    for field in [
        "plugin_id",
        "release_id",
        "component_key",
        "managed_by_plugin",
        "immutable_from_release",
    ] {
        snapshot.as_object_mut().expect("object").remove(field);
    }
    let legacy: McpRecord = serde_json::from_value(snapshot).expect("decode legacy MCP");
    assert_eq!(legacy.plugin_component, PluginComponentOwnership::default());
}
