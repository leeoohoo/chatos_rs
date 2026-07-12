// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn list_skills(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ListResourcesQuery>,
) -> Result<Json<ListResponse<SkillRecord>>, ApiError> {
    state
        .store
        .list_skills(&user, &query)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<SkillPayload>,
) -> Result<Json<SkillRecord>, ApiError> {
    let visibility = normalize_visibility(payload.visibility.as_deref(), &user)?;
    let owner_user_id = requested_owner_user_id(payload.owner_user_id.as_deref(), &user)?;
    let name = required_text(payload.name.as_deref(), "name")?;
    let display_name = payload
        .display_name
        .as_deref()
        .and_then(|value| normalized(Some(value)))
        .unwrap_or_else(|| name.clone());
    let content = payload
        .content
        .ok_or_else(|| ApiError::bad_request("content is required"))?;
    validate_skill_content(&content)?;
    let now = now_rfc3339();
    let record = SkillRecord {
        id: Uuid::new_v4().to_string(),
        owner_user_id,
        owner_kind: owner_kind_for(&visibility, &user),
        visibility,
        source_kind: default_source_kind(payload.source_kind, &user),
        name,
        display_name,
        description: payload
            .description
            .and_then(|value| normalized(Some(&value))),
        enabled: payload.enabled.unwrap_or(true),
        content,
        metadata: payload.metadata.unwrap_or_default(),
        created_by: user.user_id.clone(),
        updated_by: user.user_id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_skill(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn get_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(skill_id): Path<String>,
) -> Result<Json<SkillRecord>, ApiError> {
    let record = state
        .store
        .get_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    Ok(Json(record))
}

pub(super) async fn update_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(skill_id): Path<String>,
    Json(payload): Json<SkillPayload>,
) -> Result<Json<SkillRecord>, ApiError> {
    let mut record = state
        .store
        .get_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
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
    if let Some(content) = payload.content {
        validate_skill_content(&content)?;
        record.content = content;
    }
    if let Some(metadata) = payload.metadata {
        record.metadata = metadata;
    }
    record.updated_by = user.user_id.clone();
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_skill(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn delete_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(skill_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let record = state
        .store
        .get_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    state
        .store
        .delete_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn check_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(skill_id): Path<String>,
) -> Result<Json<ResourceCheckRecord>, ApiError> {
    let record = state
        .store
        .get_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    let check = check_record_for_skill(&record);
    state
        .store
        .replace_check(&check)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(check))
}
