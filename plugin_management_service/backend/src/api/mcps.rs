// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn list_mcps(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ListResourcesQuery>,
) -> Result<Json<ListResponse<McpRecord>>, ApiError> {
    state
        .store
        .list_mcps(&user, &query)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<McpPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    validate_client_managed_mcp_payload(&payload, &user)?;
    let visibility = normalize_visibility(payload.visibility.as_deref(), &user)?;
    let owner_user_id = requested_owner_user_id(payload.owner_user_id.as_deref(), &user)?;
    let name = required_text(payload.name.as_deref(), "name")?;
    let display_name = payload
        .display_name
        .as_deref()
        .and_then(|value| normalized(Some(value)))
        .unwrap_or_else(|| name.clone());
    let runtime = payload
        .runtime
        .ok_or_else(|| ApiError::bad_request("runtime is required"))?;
    validate_mcp_runtime(&runtime)?;
    validate_mcp_visibility_for_runtime(visibility.as_str(), &runtime)?;
    let now = now_rfc3339();
    let record = McpRecord {
        id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.clone(),
        owner_kind: owner_kind_for(&visibility, &user),
        visibility,
        source_kind: default_source_kind(payload.source_kind, &user),
        name,
        display_name,
        description: payload
            .description
            .and_then(|value| normalized(Some(&value))),
        enabled: payload.enabled.unwrap_or(true),
        runtime,
        security: payload.security.unwrap_or_default(),
        metadata: payload.metadata.unwrap_or_default(),
        created_by: user.user_id.clone(),
        updated_by: user.user_id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_mcp(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn get_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<Json<McpRecord>, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    Ok(Json(record))
}

pub(super) async fn update_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
    Json(payload): Json<McpPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    let mut record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if record.source_kind == SOURCE_KIND_SYSTEM_SEED {
        validate_system_seed_mcp_update(&payload)?;
        if let Some(enabled) = payload.enabled {
            record.enabled = enabled;
        }
        record.updated_by = user.user_id.clone();
        record.updated_at = now_rfc3339();
        state
            .store
            .replace_mcp(&record)
            .await
            .map_err(ApiError::internal)?;
        return Ok(Json(record));
    }
    validate_client_managed_mcp_payload(&payload, &user)?;
    if let Some(owner_user_id) = payload.owner_user_id.as_deref() {
        record.owner_user_id = requested_owner_user_id(Some(owner_user_id), &user)?;
    }
    if let Some(visibility) = payload.visibility.as_deref() {
        record.visibility = normalize_visibility(Some(visibility), &user)?;
        record.owner_kind = owner_kind_for(record.visibility.as_str(), &user);
    }
    if let Some(source_kind) = payload.source_kind {
        if user.is_super_admin() {
            record.source_kind = source_kind;
        }
    }
    if let Some(name) = payload.name.as_deref() {
        record.name = required_text(Some(name), "name")?;
    }
    if let Some(display_name) = payload.display_name {
        record.display_name =
            normalized(Some(&display_name)).unwrap_or_else(|| record.name.clone());
    }
    if let Some(description) = payload.description {
        record.description = normalized(Some(&description));
    }
    if let Some(enabled) = payload.enabled {
        record.enabled = enabled;
    }
    if let Some(runtime) = payload.runtime {
        validate_mcp_runtime(&runtime)?;
        record.runtime = runtime;
    }
    validate_client_managed_mcp_runtime(&record.runtime, &user)?;
    if let Some(security) = payload.security {
        record.security = security;
    }
    if let Some(metadata) = payload.metadata {
        record.metadata = metadata;
    }
    validate_mcp_visibility_for_runtime(record.visibility.as_str(), &record.runtime)?;
    record.updated_by = user.user_id.clone();
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_mcp(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn delete_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if record.source_kind == SOURCE_KIND_SYSTEM_SEED {
        record.enabled = false;
        record.updated_at = now_rfc3339();
        record.updated_by = user.user_id;
        state
            .store
            .replace_mcp(&record)
            .await
            .map_err(ApiError::internal)?;
    } else {
        state
            .store
            .delete_mcp(mcp_id.as_str())
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn check_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<Json<ResourceCheckRecord>, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if matches!(
        record.runtime.kind.as_str(),
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
            | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
            | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
    ) {
        if let Some(check) = state
            .store
            .get_check(RESOURCE_KIND_MCP, record.id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            return Ok(Json(check));
        }
    }
    let check = check_record_for_mcp(&record);
    state
        .store
        .replace_check(&check)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(check))
}
