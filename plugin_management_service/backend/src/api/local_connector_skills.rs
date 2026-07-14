// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn list_user_skill_catalog_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LocalConnectorSkillInternalQuery>,
) -> Result<Json<UserSkillCatalogResponse>, ApiError> {
    require_local_connector_internal_request(&state, &headers, LOCAL_CONNECTOR_READ_SCOPE)?;
    let owner_user_id = required_text(query.owner_user_id.as_deref(), "owner_user_id")?;
    let skills = state
        .store
        .list_internal_bundle_skills()
        .await
        .map_err(ApiError::internal)?;
    let mut items = Vec::with_capacity(skills.len());
    for skill in skills {
        let preference = state
            .store
            .get_user_skill_preference(owner_user_id.as_str(), skill.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        let (available, status, reason, installation) =
            availability_for_skill(&state, &skill, owner_user_id.as_str()).await?;
        items.push(UserSkillCatalogItem {
            skill,
            user_enabled: preference.is_some_and(|record| record.enabled),
            available,
            status,
            reason,
            installation,
        });
    }
    Ok(Json(UserSkillCatalogResponse {
        total: items.len() as u64,
        items,
    }))
}

pub(super) async fn update_user_skill_preference_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(skill_id): Path<String>,
    Json(payload): Json<UpdateUserSkillPreferencePayload>,
) -> Result<Json<UserSkillCatalogItem>, ApiError> {
    require_local_connector_internal_request(&state, &headers, LOCAL_CONNECTOR_WRITE_SCOPE)?;
    let owner_user_id = required_text(Some(payload.owner_user_id.as_str()), "owner_user_id")?;
    let skill = state
        .store
        .get_skill(skill_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Skill not found"))?;
    ensure_internal_bundle_skill(&skill)?;
    let (available, status, reason, installation) =
        availability_for_skill(&state, &skill, owner_user_id.as_str()).await?;
    if payload.enabled && !available {
        return Err(ApiError::conflict(
            reason
                .clone()
                .unwrap_or_else(|| format!("Skill is not available: {status}")),
        ));
    }
    let now = now_rfc3339();
    let existing = state
        .store
        .get_user_skill_preference(owner_user_id.as_str(), skill.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let preference = UserSkillPreferenceRecord {
        id: existing
            .as_ref()
            .map(|record| record.id.clone())
            .unwrap_or_else(|| format!("{}:{}", owner_user_id, skill.id)),
        owner_user_id,
        skill_id: skill.id.clone(),
        enabled: payload.enabled,
        enabled_at: if payload.enabled {
            existing
                .as_ref()
                .and_then(|record| record.enabled_at.clone())
                .or_else(|| Some(now.clone()))
        } else {
            None
        },
        updated_at: now,
    };
    state
        .store
        .replace_user_skill_preference(&preference)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(UserSkillCatalogItem {
        skill,
        user_enabled: preference.enabled,
        available,
        status,
        reason,
        installation,
    }))
}

pub(super) async fn sync_skill_inventory_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LocalConnectorSkillInventoryPayload>,
) -> Result<Json<Vec<SkillInstallationRecord>>, ApiError> {
    require_local_connector_internal_request(&state, &headers, LOCAL_CONNECTOR_WRITE_SCOPE)?;
    let owner_user_id = required_text(Some(payload.owner_user_id.as_str()), "owner_user_id")?;
    let device_id = required_text(Some(payload.device_id.as_str()), "device_id")?;
    let platform = required_text(Some(payload.platform.as_str()), "platform")?;
    if payload.items.len() > 200 {
        return Err(ApiError::bad_request(
            "local connector Skill inventory exceeds 200 items",
        ));
    }
    let now = now_rfc3339();
    let mut records = Vec::with_capacity(payload.items.len());
    for item in payload.items {
        let skill = state
            .store
            .get_skill(item.skill_id.as_str())
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::bad_request(format!("unknown Skill: {}", item.skill_id)))?;
        ensure_internal_bundle_skill(&skill)?;
        ensure_inventory_matches_skill(&skill, &item)?;
        let status = normalize_skill_inventory_status(item.status.as_str())?;
        let dependency_status = normalize_skill_dependency_status(item.dependency_status.as_str())?;
        records.push(SkillInstallationRecord {
            id: format!("{}:{}:{}", owner_user_id, device_id, skill.id),
            owner_user_id: owner_user_id.clone(),
            device_id: device_id.clone(),
            skill_id: skill.id,
            bundle_id: item.bundle_id,
            version: item.version,
            bundle_hash: item.bundle_hash,
            platform: platform.clone(),
            status: status.to_string(),
            dependency_status: dependency_status.to_string(),
            last_error: item
                .last_error
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .map(|value| truncate_text(value.as_str(), 1000)),
            last_checked_at: now.clone(),
        });
    }
    state
        .store
        .replace_device_skill_installations(owner_user_id.as_str(), device_id.as_str(), &records)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(records))
}

fn ensure_internal_bundle_skill(skill: &SkillRecord) -> Result<(), ApiError> {
    if skill.visibility == VISIBILITY_SYSTEM_PRIVATE
        && skill.content.kind == SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE
    {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "Skill is not an internal Local Connector bundle",
        ))
    }
}

fn ensure_inventory_matches_skill(
    skill: &SkillRecord,
    item: &LocalConnectorSkillInventoryItem,
) -> Result<(), ApiError> {
    let expected_bundle_id = required_text(skill.content.bundle_id.as_deref(), "bundle_id")?;
    let expected_version =
        required_text(skill.content.bundle_version.as_deref(), "bundle_version")?;
    if item.bundle_id.trim() != expected_bundle_id || item.version.trim() != expected_version {
        return Err(ApiError::conflict(format!(
            "Skill bundle does not match catalog: {}",
            skill.id
        )));
    }
    if item.bundle_hash.trim().is_empty() {
        return Err(ApiError::bad_request("bundle_hash is required"));
    }
    if let Some(expected_hash) = skill
        .content
        .bundle_hash
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if item.bundle_hash.trim() != expected_hash {
            return Err(ApiError::conflict(format!(
                "Skill bundle hash does not match catalog: {}",
                skill.id
            )));
        }
    }
    Ok(())
}

fn normalize_skill_inventory_status(value: &str) -> Result<&'static str, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "available" => Ok("available"),
        "unavailable" => Ok("unavailable"),
        "unsupported" => Ok("unsupported"),
        "error" => Ok("error"),
        _ => Err(ApiError::bad_request(
            "Skill inventory status must be available, unavailable, unsupported, or error",
        )),
    }
}

fn normalize_skill_dependency_status(value: &str) -> Result<&'static str, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "available" => Ok("available"),
        "missing" | "missing_dependency" => Ok("missing"),
        "unsupported" => Ok("unsupported"),
        "error" => Ok("error"),
        _ => Err(ApiError::bad_request(
            "Skill dependency status must be available, missing, unsupported, or error",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_skill_dependency_status;

    #[test]
    fn dependency_status_accepts_canonical_and_legacy_missing_values() {
        assert_eq!(
            normalize_skill_dependency_status("missing").expect("canonical missing status"),
            "missing"
        );
        assert_eq!(
            normalize_skill_dependency_status("missing_dependency").expect("legacy missing status"),
            "missing"
        );
    }
}
