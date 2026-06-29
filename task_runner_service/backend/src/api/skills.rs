use super::*;

pub(super) async fn list_skills(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(filters): Query<SkillListFilters>,
) -> Result<Json<Vec<SkillRecord>>, ApiError> {
    let skills = state
        .skill_service
        .list_skills(filters)
        .await
        .map_err(ApiError::bad_request)?;
    let skills = skills
        .into_iter()
        .filter(|skill| skill.source != SkillSource::Bundled)
        .filter(|skill| {
            owned_resource_visible_to_user(
                resource_owner_or_creator(
                    skill.owner_user_id.as_deref(),
                    skill.creator_user_id.as_deref(),
                ),
                &current_user,
            )
            .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    Ok(Json(skills))
}

pub(super) async fn list_bundled_skills(
    State(state): State<AppState>,
    Extension(_current_user): Extension<CurrentUser>,
) -> Result<Json<Vec<SkillRecord>>, ApiError> {
    let skills = state
        .skill_service
        .list_bundled_skills()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(skills))
}

pub(super) async fn create_skill(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CreateSkillRequest>,
) -> Result<(StatusCode, Json<SkillRecord>), ApiError> {
    let skill = state
        .skill_service
        .create_skill(input, &current_user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(skill)))
}

pub(super) async fn get_skill(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<SkillRecord>, ApiError> {
    let skill = state
        .skill_service
        .get_skill(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("Skill 不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            skill.owner_user_id.as_deref(),
            skill.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
    Ok(Json(skill))
}

pub(super) async fn update_skill(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateSkillRequest>,
) -> Result<Json<SkillRecord>, ApiError> {
    let existing = state
        .skill_service
        .get_skill(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("Skill 不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            existing.owner_user_id.as_deref(),
            existing.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
    let skill = state
        .skill_service
        .update_skill(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("Skill 不存在: {id}")))?;
    Ok(Json(skill))
}

pub(super) async fn delete_skill(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    let existing = state
        .skill_service
        .get_skill(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("Skill 不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            existing.owner_user_id.as_deref(),
            existing.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
    if state
        .skill_service
        .delete_skill(&id)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("Skill 不存在: {id}")))
    }
}

pub(super) async fn search_skill_marketplace(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<SkillMarketplaceQuery>,
) -> Result<Json<PaginatedResponse<SkillMarketplaceEntry>>, ApiError> {
    let mut page = state
        .skill_service
        .search_marketplace(query)
        .await
        .map_err(ApiError::bad_request)?;
    let current_owner_user_id = effective_owner_user_id(&current_user)?;
    let current_owner_skills = state
        .skill_service
        .list_skills(SkillListFilters::default())
        .await
        .map_err(ApiError::bad_request)?
        .into_iter()
        .filter(|skill| {
            resource_owner_or_creator(
                skill.owner_user_id.as_deref(),
                skill.creator_user_id.as_deref(),
            ) == Some(current_owner_user_id.as_str())
        })
        .collect::<Vec<_>>();

    page.items = page
        .items
        .into_iter()
        .map(|mut entry| {
            let installed_skill = current_owner_skills.iter().find(|skill| {
                skill.source_registry.as_deref() == Some(entry.registry.as_str())
                    && skill.source_package_id.as_deref() == Some(entry.package_id.as_str())
            });
            entry.installed_skill_id = installed_skill.map(|skill| skill.id.clone());
            entry.installed = installed_skill.is_some();
            if let Some(skill) = installed_skill {
                entry.package_file_count = skill.package_file_count;
                entry.package_total_bytes = skill.package_total_bytes;
            }
            entry
        })
        .collect::<Vec<_>>();
    Ok(Json(page))
}

pub(super) async fn install_skill_from_marketplace(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<InstallSkillRequest>,
) -> Result<(StatusCode, Json<SkillRecord>), ApiError> {
    let skill = state
        .skill_service
        .install_marketplace_skill(input, &current_user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(skill)))
}
