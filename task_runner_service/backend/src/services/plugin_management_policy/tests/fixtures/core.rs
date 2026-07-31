// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    AgentBindingRecord, BindingConditions, McpRecord as PluginMcpRecord, McpRuntime, ResolvedMcp,
    ResolvedSkill, ResourceMetadata, ResourceSecurity, SkillContent, SkillInstallationRecord,
    SkillRecord, SystemAgentKey,
};

use super::super::super::BUILTIN_RUNTIME_KIND;

pub(in super::super) fn resolved_mcp(
    id: &str,
    runtime_kind: &str,
    builtin_kind: Option<&str>,
    required: bool,
    available: bool,
) -> ResolvedMcp {
    ResolvedMcp {
        resource: PluginMcpRecord {
            id: id.to_string(),
            owner_user_id: "owner-1".to_string(),
            owner_kind: "system".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "system_seed".to_string(),
            name: id.to_string(),
            display_name: id.to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime {
                kind: runtime_kind.to_string(),
                system_key: (runtime_kind == chatos_plugin_management_sdk::SYSTEM_MCP_RUNTIME_KIND)
                    .then(|| builtin_kind.map(ToOwned::to_owned))
                    .flatten(),
                builtin_kind: (runtime_kind == BUILTIN_RUNTIME_KIND)
                    .then(|| builtin_kind.map(ToOwned::to_owned))
                    .flatten(),
                url: (runtime_kind == "http").then(|| "http://127.0.0.1/mcp".to_string()),
                ..McpRuntime::default()
            },
            security: ResourceSecurity::default(),
            metadata: ResourceMetadata::default(),
            plugin_component: Default::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        binding: AgentBindingRecord {
            id: format!("binding-{id}"),
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
            binding_scope: if required {
                "system_required".to_string()
            } else {
                "global_default".to_string()
            },
            owner_user_id: None,
            resource_kind: "mcp".to_string(),
            resource_id: id.to_string(),
            enabled: true,
            required,
            priority: 0,
            conditions: BindingConditions::default(),
            component_allowlist: Vec::new(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available,
        status: if available { "available" } else { "offline" }.to_string(),
        reason: (!available).then(|| "offline".to_string()),
    }
}

pub(in super::super) fn resolved_skill(id: &str, required: bool, available: bool) -> ResolvedSkill {
    ResolvedSkill {
        resource: SkillRecord {
            id: id.to_string(),
            owner_user_id: "system".to_string(),
            owner_kind: "admin".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "admin_created".to_string(),
            name: "remotion-best-practices".to_string(),
            display_name: "Remotion Best Practices".to_string(),
            description: Some("Local prompt-only Skill".to_string()),
            enabled: true,
            content: SkillContent {
                kind: "local_connector_bundle".to_string(),
                bundle_id: Some("chatos.internal.remotion-best-practices".to_string()),
                bundle_version: Some("1.0.0".to_string()),
                bundle_hash: Some("bundle-hash-1".to_string()),
                entrypoint_kind: Some("prompt_only".to_string()),
                ..SkillContent::default()
            },
            metadata: ResourceMetadata::default(),
            plugin_component: Default::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        binding: AgentBindingRecord {
            id: format!("binding-{id}"),
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
            binding_scope: if required {
                "system_required".to_string()
            } else {
                "global_default".to_string()
            },
            owner_user_id: None,
            resource_kind: "skill".to_string(),
            resource_id: id.to_string(),
            enabled: true,
            required,
            priority: 0,
            conditions: BindingConditions::default(),
            component_allowlist: Vec::new(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available,
        status: if available { "available" } else { "offline" }.to_string(),
        reason: (!available).then(|| "offline".to_string()),
        installation: available.then(|| SkillInstallationRecord {
            id: format!("owner-1:device-1:{id}"),
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            skill_id: id.to_string(),
            bundle_id: "chatos.internal.remotion-best-practices".to_string(),
            version: "1.0.0".to_string(),
            bundle_hash: "bundle-hash-1".to_string(),
            platform: "macos-arm64".to_string(),
            status: "available".to_string(),
            dependency_status: "available".to_string(),
            last_error: None,
            last_checked_at: "now".to_string(),
        }),
    }
}
