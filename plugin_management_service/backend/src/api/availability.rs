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
    let is_local = matches!(
        record.content.kind.as_str(),
        SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE
            | "local_connector_file"
            | "local_connector_package"
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
    owner_user_id: &str,
) -> Result<
    (
        bool,
        String,
        Option<String>,
        Option<SkillInstallationRecord>,
    ),
    ApiError,
> {
    if !record.enabled {
        return Ok((
            false,
            "unavailable".to_string(),
            Some("resource is disabled".to_string()),
            None,
        ));
    }
    if record.content.kind == SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE {
        let installation = state
            .store
            .get_skill_installation(owner_user_id, record.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        let Some(installation) = installation else {
            return Ok((
                false,
                "not_installed".to_string(),
                Some(
                    "Skill bundle has not been reported by the active Local Connector".to_string(),
                ),
                None,
            ));
        };
        let expected_bundle_id = record
            .content
            .bundle_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let expected_version = record
            .content
            .bundle_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let expected_hash = record
            .content
            .bundle_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if expected_bundle_id != Some(installation.bundle_id.as_str())
            || expected_version.is_some_and(|value| value != installation.version)
            || expected_hash.is_some_and(|value| value != installation.bundle_hash)
        {
            return Ok((
                false,
                "version_mismatch".to_string(),
                Some("Local Connector Skill bundle does not match the admin catalog".to_string()),
                Some(installation),
            ));
        }
        let fresh = local_connector_check_is_fresh(
            installation.last_checked_at.as_str(),
            state.config.local_connector_check_ttl,
        );
        let available = installation.status == "available"
            && installation.dependency_status == "available"
            && fresh;
        let status = if available {
            "available".to_string()
        } else if !fresh {
            "offline".to_string()
        } else {
            installation.status.clone()
        };
        let reason = if available {
            installation.last_error.clone()
        } else if !fresh {
            Some("Local Connector Skill inventory is stale".to_string())
        } else {
            installation.last_error.clone().or_else(|| {
                Some(format!(
                    "Skill dependency status is {}",
                    installation.dependency_status
                ))
            })
        };
        return Ok((available, status, reason, Some(installation)));
    }
    let local = matches!(
        record.content.kind.as_str(),
        SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE
            | "local_connector_file"
            | "local_connector_package"
    );
    if local {
        let check = state
            .store
            .get_check(RESOURCE_KIND_SKILL, record.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        return Ok(match check {
            Some(check) if check.status == "available" => {
                (true, check.status, check.last_error, None)
            }
            Some(check) => (false, check.status, check.last_error, None),
            None => (
                false,
                "unknown".to_string(),
                Some("Local Connector status has not been checked".to_string()),
                None,
            ),
        });
    }
    Ok((true, "available".to_string(), None, None))
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
    installation: Option<&SkillInstallationRecord>,
) {
    let local = resource.content.local_connector.as_ref();
    let device_id = installation
        .map(|item| item.device_id.clone())
        .or_else(|| local.and_then(|item| item.device_id.clone()));
    if resource.content.kind != SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE && local.is_none() {
        return;
    }
    out.push(LocalConnectorRequirement {
        resource_kind: RESOURCE_KIND_SKILL.to_string(),
        resource_id: resource.id.clone(),
        device_id,
        workspace_id: local.and_then(|item| item.workspace_id.clone()),
        required: binding.required,
        available,
        reason,
    });
}
