// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn list_skill_packages(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ListResourcesQuery>,
) -> Result<Json<ListResponse<SkillPackageRecord>>, ApiError> {
    state
        .store
        .list_skill_packages(&user, &query)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_skill_package(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<SkillPackagePayload>,
) -> Result<Json<SkillPackageRecord>, ApiError> {
    let visibility = normalize_visibility(payload.visibility.as_deref(), &user)?;
    let owner_user_id = requested_owner_user_id(payload.owner_user_id.as_deref(), &user)?;
    let name = required_text(payload.name.as_deref(), "name")?;
    let now = now_rfc3339();
    let record = SkillPackageRecord {
        id: Uuid::new_v4().to_string(),
        owner_user_id,
        visibility,
        source_kind: default_source_kind(payload.source_kind, &user),
        name,
        description: payload
            .description
            .and_then(|value| normalized(Some(&value))),
        repository: payload
            .repository
            .and_then(|value| normalized(Some(&value))),
        branch: payload.branch.and_then(|value| normalized(Some(&value))),
        cache_ref: payload.cache_ref.and_then(|value| normalized(Some(&value))),
        local_connector: payload.local_connector,
        skill_ids: payload.skill_ids.unwrap_or_default(),
        installed: payload.installed.unwrap_or(true),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_skill_package(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn get_skill_package(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(package_id): Path<String>,
) -> Result<Json<SkillPackageRecord>, ApiError> {
    let record = state
        .store
        .get_skill_package(package_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill package not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    Ok(Json(record))
}

pub(super) async fn update_skill_package(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(package_id): Path<String>,
    Json(payload): Json<SkillPackagePayload>,
) -> Result<Json<SkillPackageRecord>, ApiError> {
    let mut record = state
        .store
        .get_skill_package(package_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill package not found"))?;
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
    }
    if let Some(source_kind) = payload.source_kind {
        if user.is_super_admin() {
            record.source_kind = source_kind;
        }
    }
    if let Some(name) = payload.name.as_deref() {
        record.name = required_text(Some(name), "name")?;
    }
    if let Some(description) = payload.description {
        record.description = normalized(Some(&description));
    }
    if let Some(repository) = payload.repository {
        record.repository = normalized(Some(&repository));
    }
    if let Some(branch) = payload.branch {
        record.branch = normalized(Some(&branch));
    }
    if let Some(cache_ref) = payload.cache_ref {
        record.cache_ref = normalized(Some(&cache_ref));
    }
    if payload.local_connector.is_some() {
        record.local_connector = payload.local_connector;
    }
    if let Some(skill_ids) = payload.skill_ids {
        record.skill_ids = skill_ids;
    }
    if let Some(installed) = payload.installed {
        record.installed = installed;
    }
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_skill_package(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn delete_skill_package(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(package_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let record = state
        .store
        .get_skill_package(package_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill package not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    state
        .store
        .delete_skill_package(package_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
