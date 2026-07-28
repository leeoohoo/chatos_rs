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
