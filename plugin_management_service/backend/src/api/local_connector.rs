// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn list_local_connector_mcps_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LocalConnectorMcpInternalQuery>,
) -> Result<Json<ListResponse<McpRecord>>, ApiError> {
    require_local_connector_internal_request(&state, &headers, LOCAL_CONNECTOR_READ_SCOPE)?;
    let owner_user_id = required_text(query.owner_user_id.as_deref(), "owner_user_id")?;
    let device_id = required_text(query.device_id.as_deref(), "device_id")?;
    let items = state
        .store
        .list_local_connector_mcps(owner_user_id.as_str(), device_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(ListResponse {
        total: items.len() as u64,
        items,
    }))
}

pub(super) async fn sync_local_connector_mcp_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LocalConnectorMcpSyncPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    require_local_connector_internal_request(&state, &headers, LOCAL_CONNECTOR_WRITE_SCOPE)?;
    let owner_user_id = required_text(Some(payload.owner_user_id.as_str()), "owner_user_id")?;
    let device_id = required_text(Some(payload.device_id.as_str()), "device_id")?;
    let manifest_id = required_text(Some(payload.manifest_id.as_str()), "manifest_id")?;
    let existing = state
        .store
        .find_local_connector_mcp(
            owner_user_id.as_str(),
            device_id.as_str(),
            manifest_id.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    sync_local_connector_mcp_record(&state, existing, payload)
        .await
        .map(Json)
}

pub(super) async fn update_local_connector_mcp_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(mcp_id): Path<String>,
    Json(payload): Json<LocalConnectorMcpSyncPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    require_local_connector_internal_request(&state, &headers, LOCAL_CONNECTOR_WRITE_SCOPE)?;
    let record = load_local_connector_mcp_for_sync(&state, mcp_id.as_str(), &payload).await?;
    sync_local_connector_mcp_record(&state, Some(record), payload)
        .await
        .map(Json)
}

pub(super) async fn delete_local_connector_mcp_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(mcp_id): Path<String>,
    Query(query): Query<LocalConnectorMcpInternalQuery>,
) -> Result<StatusCode, ApiError> {
    require_local_connector_internal_request(&state, &headers, LOCAL_CONNECTOR_WRITE_SCOPE)?;
    let owner_user_id = required_text(query.owner_user_id.as_deref(), "owner_user_id")?;
    let device_id = required_text(query.device_id.as_deref(), "device_id")?;
    let manifest_id = required_text(query.manifest_id.as_deref(), "manifest_id")?;
    let record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_local_connector_record_scope(
        &record,
        owner_user_id.as_str(),
        device_id.as_str(),
        manifest_id.as_str(),
    )?;
    state
        .store
        .delete_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn update_local_connector_mcp_status_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(mcp_id): Path<String>,
    Json(payload): Json<LocalConnectorMcpStatusPayload>,
) -> Result<Json<ResourceCheckRecord>, ApiError> {
    require_local_connector_internal_request(&state, &headers, LOCAL_CONNECTOR_WRITE_SCOPE)?;
    update_local_connector_mcp_status_record(&state, mcp_id.as_str(), payload)
        .await
        .map(Json)
}

pub(super) async fn update_local_connector_mcp_status_batch_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LocalConnectorMcpStatusBatchPayload>,
) -> Result<Json<Vec<ResourceCheckRecord>>, ApiError> {
    require_local_connector_internal_request(&state, &headers, LOCAL_CONNECTOR_WRITE_SCOPE)?;
    if payload.items.len() > 200 {
        return Err(ApiError::bad_request(
            "local connector MCP status batch exceeds 200 items",
        ));
    }
    let mut checks = Vec::with_capacity(payload.items.len());
    for item in payload.items {
        checks.push(
            update_local_connector_mcp_status_record(
                &state,
                item.mcp_id.as_str(),
                LocalConnectorMcpStatusPayload {
                    owner_user_id: item.owner_user_id,
                    device_id: item.device_id,
                    workspace_id: item.workspace_id,
                    manifest_id: item.manifest_id,
                    status: item.status,
                    last_error: item.last_error,
                    tool_snapshot: item.tool_snapshot,
                    manifest_hash: item.manifest_hash,
                },
            )
            .await?,
        );
    }
    Ok(Json(checks))
}

pub(super) async fn sync_local_connector_mcp_record(
    state: &AppState,
    existing: Option<McpRecord>,
    payload: LocalConnectorMcpSyncPayload,
) -> Result<McpRecord, ApiError> {
    validate_local_connector_sync_payload(&payload)?;
    let owner_user_id = required_text(Some(payload.owner_user_id.as_str()), "owner_user_id")?;
    let device_id = required_text(Some(payload.device_id.as_str()), "device_id")?;
    let manifest_id = required_text(Some(payload.manifest_id.as_str()), "manifest_id")?;
    let internal_name = required_text(Some(payload.internal_name.as_str()), "internal_name")?;
    let display_name = required_text(Some(payload.display_name.as_str()), "display_name")?;
    validate_internal_mcp_name(internal_name.as_str())?;
    let now = now_rfc3339();
    let mut metadata = existing
        .as_ref()
        .map(|record| record.metadata.clone())
        .unwrap_or_default();
    metadata.category = Some("user_local_mcp".to_string());
    metadata
        .extra
        .insert("managed_by".to_string(), json!("local_connector_client"));
    let runtime = McpRuntime {
        kind: payload.runtime_kind.clone(),
        server_name: Some(internal_name.clone()),
        local_connector: Some(LocalConnectorRef {
            device_id: Some(device_id.clone()),
            workspace_id: None,
            manifest_id: Some(manifest_id.clone()),
            relative_path: None,
            requires_online: true,
        }),
        ..McpRuntime::default()
    };
    validate_mcp_runtime(&runtime)?;
    let record = McpRecord {
        id: existing
            .as_ref()
            .map(|record| record.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        owner_user_id: owner_user_id.clone(),
        owner_kind: OWNER_KIND_USER.to_string(),
        visibility: VISIBILITY_PRIVATE.to_string(),
        source_kind: SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED.to_string(),
        name: existing
            .as_ref()
            .map(|record| record.name.clone())
            .unwrap_or_else(|| internal_name.clone()),
        display_name,
        description: payload
            .description
            .as_deref()
            .and_then(|value| normalized(Some(value))),
        enabled: payload.enabled,
        runtime,
        security: existing
            .as_ref()
            .map(|record| record.security.clone())
            .unwrap_or_default(),
        metadata,
        created_by: existing
            .as_ref()
            .map(|record| record.created_by.clone())
            .unwrap_or_else(|| "local-connector-service".to_string()),
        updated_by: "local-connector-service".to_string(),
        created_at: existing
            .as_ref()
            .map(|record| record.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    if let Some(existing) = existing.as_ref() {
        ensure_local_connector_record_scope(
            existing,
            owner_user_id.as_str(),
            device_id.as_str(),
            manifest_id.as_str(),
        )?;
    }
    state
        .store
        .replace_mcp(&record)
        .await
        .map_err(ApiError::internal)?;
    reconcile_local_connector_check_after_sync(state, &record, payload.manifest_hash.as_deref())
        .await?;
    Ok(record)
}

pub(super) async fn load_local_connector_mcp_for_sync(
    state: &AppState,
    mcp_id: &str,
    payload: &LocalConnectorMcpSyncPayload,
) -> Result<McpRecord, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_local_connector_record_scope(
        &record,
        payload.owner_user_id.as_str(),
        payload.device_id.as_str(),
        payload.manifest_id.as_str(),
    )?;
    Ok(record)
}

pub(super) fn validate_local_connector_sync_payload(
    payload: &LocalConnectorMcpSyncPayload,
) -> Result<(), ApiError> {
    for (value, field) in [
        (payload.owner_user_id.as_str(), "owner_user_id"),
        (payload.device_id.as_str(), "device_id"),
        (payload.manifest_id.as_str(), "manifest_id"),
        (payload.internal_name.as_str(), "internal_name"),
        (payload.display_name.as_str(), "display_name"),
    ] {
        required_text(Some(value), field)?;
    }
    if !matches!(
        payload.runtime_kind.as_str(),
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
    ) {
        return Err(ApiError::bad_request(
            "local connector user MCP runtime must be local_connector_stdio or local_connector_http",
        ));
    }
    Ok(())
}

pub(super) fn validate_internal_mcp_name(value: &str) -> Result<(), ApiError> {
    if value.len() > 96
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return Err(ApiError::bad_request(
            "internal_name must contain only ASCII letters, numbers, underscore, or hyphen",
        ));
    }
    Ok(())
}

pub(super) fn ensure_local_connector_record_scope(
    record: &McpRecord,
    owner_user_id: &str,
    device_id: &str,
    manifest_id: &str,
) -> Result<(), ApiError> {
    let local = record.runtime.local_connector.as_ref();
    let matches_scope = record.owner_user_id == owner_user_id
        && record.visibility == VISIBILITY_PRIVATE
        && record.source_kind == SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED
        && matches!(
            record.runtime.kind.as_str(),
            RUNTIME_KIND_LOCAL_CONNECTOR_STDIO | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
        )
        && local.and_then(|value| value.device_id.as_deref()) == Some(device_id)
        && local.and_then(|value| value.manifest_id.as_deref()) == Some(manifest_id);
    if matches_scope {
        Ok(())
    } else {
        Err(ApiError::not_found("MCP not found"))
    }
}

pub(super) async fn reconcile_local_connector_check_after_sync(
    state: &AppState,
    record: &McpRecord,
    manifest_hash: Option<&str>,
) -> Result<(), ApiError> {
    let current = state
        .store
        .get_check(RESOURCE_KIND_MCP, record.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let normalized_hash = normalized(manifest_hash);
    let preserve_available = record.enabled
        && normalized_hash.is_some()
        && current.as_ref().is_some_and(|check| {
            check.status == "available" && check.manifest_hash == normalized_hash
        });
    if preserve_available {
        return Ok(());
    }
    let check = ResourceCheckRecord {
        id: format!("{}:{}", RESOURCE_KIND_MCP, record.id),
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: record.id.clone(),
        owner_user_id: record.owner_user_id.clone(),
        status: if record.enabled {
            "unknown".to_string()
        } else {
            "unavailable".to_string()
        },
        last_checked_at: now_rfc3339(),
        last_error: Some(if record.enabled {
            "Local Connector MCP is waiting for a successful local check".to_string()
        } else {
            "resource is disabled".to_string()
        }),
        tool_snapshot: Vec::new(),
        manifest_hash: normalized_hash,
    };
    state
        .store
        .replace_check(&check)
        .await
        .map_err(ApiError::internal)
}

pub(super) async fn update_local_connector_mcp_status_record(
    state: &AppState,
    mcp_id: &str,
    payload: LocalConnectorMcpStatusPayload,
) -> Result<ResourceCheckRecord, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_local_connector_record_scope(
        &record,
        payload.owner_user_id.as_str(),
        payload.device_id.as_str(),
        payload.manifest_id.as_str(),
    )?;
    let status = normalize_local_connector_status(payload.status.as_str())?;
    let manifest_hash = normalized(payload.manifest_hash.as_deref());
    if status == "available" {
        if !record.enabled {
            return Err(ApiError::bad_request(
                "disabled Local Connector MCP cannot be marked available",
            ));
        }
        if manifest_hash.is_none() {
            return Err(ApiError::bad_request(
                "available Local Connector MCP requires manifest_hash",
            ));
        }
        if payload.tool_snapshot.is_empty() {
            return Err(ApiError::bad_request(
                "available Local Connector MCP requires a non-empty tool snapshot",
            ));
        }
    }
    let current = state
        .store
        .get_check(RESOURCE_KIND_MCP, record.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    ensure_local_connector_manifest_hash_matches(current.as_ref(), manifest_hash.as_deref())?;
    let tool_snapshot = sanitize_tool_snapshot(
        payload.tool_snapshot,
        state.config.local_connector_max_tool_snapshot_bytes,
    )?;
    let check = ResourceCheckRecord {
        id: format!("{}:{}", RESOURCE_KIND_MCP, record.id),
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: record.id.clone(),
        owner_user_id: record.owner_user_id.clone(),
        status: if record.enabled {
            status.to_string()
        } else {
            "unavailable".to_string()
        },
        last_checked_at: now_rfc3339(),
        last_error: normalized(payload.last_error.as_deref())
            .map(|value| truncate_text(value.as_str(), 1000)),
        tool_snapshot,
        manifest_hash,
    };
    state
        .store
        .replace_check(&check)
        .await
        .map_err(ApiError::internal)?;
    Ok(check)
}

pub(super) fn ensure_local_connector_manifest_hash_matches(
    current: Option<&ResourceCheckRecord>,
    manifest_hash: Option<&str>,
) -> Result<(), ApiError> {
    if current
        .and_then(|check| check.manifest_hash.as_deref())
        .is_some()
        && manifest_hash.is_some()
        && current.and_then(|check| check.manifest_hash.as_deref()) != manifest_hash
    {
        return Err(ApiError::conflict(
            "Local Connector MCP manifest hash does not match the synced descriptor",
        ));
    }
    Ok(())
}

pub(super) fn normalize_local_connector_status(value: &str) -> Result<&'static str, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "available" => Ok("available"),
        "unavailable" => Ok("unavailable"),
        "offline" => Ok("offline"),
        "invalid" => Ok("invalid"),
        "unknown" => Ok("unknown"),
        _ => Err(ApiError::bad_request(
            "status must be available, unavailable, offline, invalid, or unknown",
        )),
    }
}

pub(super) fn sanitize_tool_snapshot(
    mut tools: Vec<serde_json::Value>,
    max_bytes: usize,
) -> Result<Vec<serde_json::Value>, ApiError> {
    if tools.len() > 200 {
        tools.truncate(200);
    }
    let encoded = serde_json::to_vec(&tools)
        .map_err(|err| ApiError::bad_request(format!("invalid tool snapshot: {err}")))?;
    if encoded.len() > max_bytes {
        return Err(ApiError::bad_request(format!(
            "tool snapshot exceeds {max_bytes} bytes"
        )));
    }
    Ok(tools)
}

pub(super) fn truncate_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}
