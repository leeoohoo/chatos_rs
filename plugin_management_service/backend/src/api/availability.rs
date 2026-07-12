// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn check_record_for_mcp(record: &McpRecord) -> ResourceCheckRecord {
    let (status, error) = if record.enabled {
        match record.runtime.kind.as_str() {
            RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
            | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
            | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY => (
                "unknown".to_string(),
                Some(
                    "Local Connector runtime check is not wired in this service phase".to_string(),
                ),
            ),
            _ => ("available".to_string(), None),
        }
    } else {
        (
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        )
    };
    ResourceCheckRecord {
        id: format!("{}:{}", RESOURCE_KIND_MCP, record.id),
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: record.id.clone(),
        owner_user_id: record.owner_user_id.clone(),
        status,
        last_checked_at: now_rfc3339(),
        last_error: error,
        tool_snapshot: Vec::new(),
        manifest_hash: None,
    }
}

pub(super) fn check_record_for_skill(record: &SkillRecord) -> ResourceCheckRecord {
    let is_local = matches!(
        record.content.kind.as_str(),
        "local_connector_file" | "local_connector_package"
    );
    let (status, error) = if !record.enabled {
        (
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        )
    } else if is_local {
        (
            "unknown".to_string(),
            Some("Local Connector skill check is not wired in this service phase".to_string()),
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
                BINDING_SCOPE_SYSTEM_REQUIRED | BINDING_SCOPE_GLOBAL_DEFAULT
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
    let local = matches!(
        record.runtime.kind.as_str(),
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
            | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
            | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
    );
    if local {
        let check = state
            .store
            .get_check(RESOURCE_KIND_MCP, record.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        return Ok(match check {
            Some(check)
                if check.status == "available"
                    && check.manifest_hash.is_some()
                    && !check.tool_snapshot.is_empty()
                    && local_connector_check_is_fresh(
                        check.last_checked_at.as_str(),
                        state.config.local_connector_check_ttl,
                    ) =>
            {
                (true, check.status, check.last_error)
            }
            Some(check) if check.status == "available" => (
                false,
                "offline".to_string(),
                Some("Local Connector availability check is stale or incomplete".to_string()),
            ),
            Some(check) => (false, check.status, check.last_error),
            None => (
                false,
                "unknown".to_string(),
                Some("Local Connector status has not been checked".to_string()),
            ),
        });
    }
    Ok((true, "available".to_string(), None))
}

pub(super) fn local_connector_check_is_fresh(
    last_checked_at: &str,
    ttl: std::time::Duration,
) -> bool {
    let Ok(last_checked_at) = chrono::DateTime::parse_from_rfc3339(last_checked_at) else {
        return false;
    };
    let age = chrono::Utc::now().signed_duration_since(last_checked_at.with_timezone(&chrono::Utc));
    age.num_milliseconds() >= 0
        && u128::try_from(age.num_milliseconds())
            .ok()
            .is_some_and(|age_ms| age_ms <= ttl.as_millis())
}

pub(super) async fn availability_for_skill(
    state: &AppState,
    record: &SkillRecord,
) -> Result<(bool, String, Option<String>), ApiError> {
    if !record.enabled {
        return Ok((
            false,
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
        ));
    }
    let local = matches!(
        record.content.kind.as_str(),
        "local_connector_file" | "local_connector_package"
    );
    if local {
        let check = state
            .store
            .get_check(RESOURCE_KIND_SKILL, record.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        return Ok(match check {
            Some(check) if check.status == "available" => (true, check.status, check.last_error),
            Some(check) => (false, check.status, check.last_error),
            None => (
                false,
                "unknown".to_string(),
                Some("Local Connector status has not been checked".to_string()),
            ),
        });
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

pub(super) fn collect_local_connector_requirement_for_skill(
    out: &mut Vec<LocalConnectorRequirement>,
    resource: &SkillRecord,
    binding: &AgentBindingRecord,
    available: bool,
    reason: Option<String>,
) {
    let Some(local) = resource.content.local_connector.as_ref() else {
        return;
    };
    out.push(LocalConnectorRequirement {
        resource_kind: RESOURCE_KIND_SKILL.to_string(),
        resource_id: resource.id.clone(),
        device_id: local.device_id.clone(),
        workspace_id: local.workspace_id.clone(),
        required: binding.required,
        available,
        reason,
    });
}
