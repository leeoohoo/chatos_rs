// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn check_record_for_mcp(
    record: &McpRecord,
    status: impl Into<String>,
    error: Option<String>,
    tool_snapshot: Vec<serde_json::Value>,
) -> ResourceCheckRecord {
    ResourceCheckRecord {
        id: format!("{}:{}", RESOURCE_KIND_MCP, record.id),
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: record.id.clone(),
        owner_user_id: record.owner_user_id.clone(),
        status: status.into(),
        last_checked_at: now_rfc3339(),
        last_error: error,
        tool_snapshot,
        manifest_hash: None,
    }
}

pub(super) fn check_record_for_skill(record: &SkillRecord) -> ResourceCheckRecord {
    let (status, error) = if !record.enabled {
        (
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        )
    } else {
        ("available".to_string(), None)
    };
    ResourceCheckRecord {
        id: format!("{}:{}", RESOURCE_KIND_SKILL, record.id),
        resource_kind: RESOURCE_KIND_SKILL.to_string(),
        resource_id: record.id.clone(),
        owner_user_id: record.owner_user_id.clone(),
        status,
        last_checked_at: now_rfc3339(),
        last_error: error,
        tool_snapshot: Vec::new(),
        manifest_hash: None,
    }
}

pub(super) fn resource_visible_in_runtime(
    owner_user_id: &str,
    visibility: &str,
    runtime_owner_user_id: &str,
    binding: &AgentBindingRecord,
) -> bool {
    visibility == VISIBILITY_PUBLIC
        || owner_user_id == runtime_owner_user_id
        || (visibility == VISIBILITY_SYSTEM_PRIVATE
            && matches!(
                binding.binding_scope.as_str(),
                BINDING_SCOPE_ADMIN_OVERRIDE
                    | BINDING_SCOPE_SYSTEM_REQUIRED
                    | BINDING_SCOPE_GLOBAL_DEFAULT
            ))
}

pub(super) async fn availability_for_mcp(
    state: &AppState,
    record: &McpRecord,
) -> Result<(bool, String, Option<String>), ApiError> {
    if !record.enabled {
        return Ok((
            false,
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        ));
    }
    let _ = state;
    Ok((true, "available".to_string(), None))
}

pub(super) async fn availability_for_skill(
    _state: &AppState,
    record: &SkillRecord,
    _owner_user_id: &str,
) -> Result<(bool, String, Option<String>), ApiError> {
    if !record.enabled {
        return Ok((
            false,
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        ));
    }
    Ok((true, "available".to_string(), None))
}

pub(super) fn collect_local_connector_requirement_for_mcp(
    out: &mut Vec<LocalConnectorRequirement>,
    resource: &McpRecord,
    binding: &AgentBindingRecord,
    available: bool,
    reason: Option<String>,
) {
    let Some(local) = resource.runtime.local_connector.as_ref() else {
        return;
    };
    out.push(LocalConnectorRequirement {
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: resource.id.clone(),
        device_id: local.device_id.clone(),
        workspace_id: local.workspace_id.clone(),
        required: binding.required,
        available,
        reason,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mcp_check_record_keeps_the_real_tool_snapshot() {
        let record = McpRecord {
            id: "mcp-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_kind: OWNER_KIND_USER.to_string(),
            visibility: VISIBILITY_PRIVATE.to_string(),
            source_kind: SOURCE_KIND_USER_CREATED.to_string(),
            name: "demo".to_string(),
            display_name: "Demo".to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime {
                kind: RUNTIME_KIND_HTTP.to_string(),
                ..McpRuntime::default()
            },
            security: ResourceSecurity::default(),
            metadata: ResourceMetadata::default(),
            plugin_component: PluginComponentOwnership::default(),
            created_by: "user-1".to_string(),
            updated_by: "user-1".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let tools = vec![json!({"name": "demo_tool", "inputSchema": {"type": "object"}})];
        let check = check_record_for_mcp(&record, "available", None, tools.clone());
        assert_eq!(check.tool_snapshot, tools);
    }
}
