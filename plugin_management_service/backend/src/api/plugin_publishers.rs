// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use super::*;
use chatos_plugin_management_sdk::PluginPublisher;

const MAX_REVIEW_NOTE_CHARS: usize = 2_000;

pub(super) async fn list_plugin_publishers(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(mut query): Query<PluginPublisherQuery>,
) -> Result<Json<ListResponse<PluginPublisherRecord>>, ApiError> {
    normalize_publisher_query(&mut query)?;
    state
        .store
        .list_plugin_publishers(&query, Some(user.effective_owner_user_id()))
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn list_admin_plugin_publishers(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(mut query): Query<PluginPublisherQuery>,
) -> Result<Json<ListResponse<PluginPublisherRecord>>, ApiError> {
    ensure_super_admin(&user)?;
    normalize_publisher_query(&mut query)?;
    state
        .store
        .list_plugin_publishers(&query, None)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn ensure_admin_managed_publisher(
    state: &AppState,
    user: &CurrentUser,
    marketplace: &PluginMarketplaceRecord,
    publisher_id: &str,
    name: Option<&str>,
    website: Option<&str>,
) -> Result<PluginPublisherRecord, ApiError> {
    ensure_super_admin(user)?;
    ensure_publisher_onboarding_marketplace(marketplace)?;
    let publisher_id = validate_plugin_identifier(publisher_id, "publisher_id")?;
    if let Some(existing) = state
        .store
        .find_plugin_publisher(marketplace.id.as_str(), publisher_id.as_str())
        .await
        .map_err(ApiError::internal)?
    {
        if existing.status != PLUGIN_PUBLISHER_STATUS_APPROVED {
            return Err(ApiError::conflict(
                "Plugin publisher exists but is not approved",
            ));
        }
        return Ok(existing);
    }

    let name = required_text(name, "publisher_name")?;
    let website = normalize_https_url(website, "publisher_website")?;
    let now = now_rfc3339();
    let record = PluginPublisherRecord {
        id: Uuid::new_v4().to_string(),
        publisher_id: publisher_id.clone(),
        marketplace_id: marketplace.id.clone(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        name,
        website,
        status: PLUGIN_PUBLISHER_STATUS_APPROVED.to_string(),
        signing_keys: Vec::new(),
        submitted_at: now.clone(),
        reviewed_at: Some(now.clone()),
        reviewed_by: Some(user.user_id.clone()),
        review_note: Some(
            "Created and approved during administrator package publication.".to_string(),
        ),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_plugin_publisher(&record)
        .await
        .map_err(|error| {
            if error.contains("E11000") {
                ApiError::conflict("Plugin publisher was created concurrently; retry publishing")
            } else {
                ApiError::internal(error)
            }
        })?;
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_REVIEW_PUBLISHER,
        user.user_id.as_str(),
        None,
        record.id.as_str(),
        None,
        "success",
        BTreeMap::from([
            ("publisher_id".to_string(), json!(record.publisher_id)),
            ("marketplace_id".to_string(), json!(record.marketplace_id)),
            (
                "decision".to_string(),
                json!(PLUGIN_PUBLISHER_DECISION_APPROVE),
            ),
            ("status".to_string(), json!(record.status)),
            ("source".to_string(), json!("package_publish")),
        ]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    Ok(record)
}

pub(super) async fn submit_plugin_publisher(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(mut payload): Json<PluginPublisherApplicationPayload>,
) -> Result<Json<PluginPublisherRecord>, ApiError> {
    normalize_publisher_application(&mut payload)?;
    let marketplace = state
        .store
        .get_plugin_marketplace(payload.marketplace_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("Plugin marketplace not found"))?;
    ensure_publisher_onboarding_marketplace(&marketplace)?;
    let owner_user_id = user.effective_owner_user_id().to_string();
    let existing = state
        .store
        .find_plugin_publisher(
            payload.marketplace_id.as_str(),
            payload.publisher_id.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    if let Some(existing) = existing.as_ref() {
        if existing.owner_user_id != owner_user_id {
            return Err(ApiError::conflict(
                "Publisher identity is already claimed in this marketplace",
            ));
        }
        match existing.status.as_str() {
            PLUGIN_PUBLISHER_STATUS_REJECTED => {}
            PLUGIN_PUBLISHER_STATUS_PENDING => {
                return Err(ApiError::conflict(
                    "Publisher application is already pending review",
                ));
            }
            PLUGIN_PUBLISHER_STATUS_APPROVED => {
                return Err(ApiError::conflict("Publisher is already approved"));
            }
            PLUGIN_PUBLISHER_STATUS_SUSPENDED => {
                return Err(ApiError::conflict(
                    "Suspended publishers require an administrator review",
                ));
            }
            _ => return Err(ApiError::conflict("Publisher status is invalid")),
        }
    }

    let now = now_rfc3339();
    let record = PluginPublisherRecord {
        id: existing
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        publisher_id: payload.publisher_id,
        marketplace_id: payload.marketplace_id,
        owner_user_id: owner_user_id.clone(),
        name: payload.name,
        website: payload.website,
        status: PLUGIN_PUBLISHER_STATUS_PENDING.to_string(),
        signing_keys: Vec::new(),
        submitted_at: now.clone(),
        reviewed_at: None,
        reviewed_by: None,
        review_note: None,
        created_at: existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    if let Some(existing) = existing.as_ref() {
        let replaced = state
            .store
            .replace_plugin_publisher_if_matches(existing, &record)
            .await
            .map_err(ApiError::internal)?;
        if !replaced {
            return Err(ApiError::conflict(
                "Publisher application changed concurrently; reload before resubmitting",
            ));
        }
    } else {
        state
            .store
            .replace_plugin_publisher(&record)
            .await
            .map_err(|error| {
                if error.contains("E11000") {
                    ApiError::conflict("Publisher identity is already claimed")
                } else {
                    ApiError::internal(error)
                }
            })?;
    }
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_SUBMIT_PUBLISHER,
        owner_user_id.as_str(),
        None,
        record.id.as_str(),
        None,
        "success",
        BTreeMap::from([
            ("publisher_id".to_string(), json!(record.publisher_id)),
            ("marketplace_id".to_string(), json!(record.marketplace_id)),
        ]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn review_admin_plugin_publisher(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(record_id): Path<String>,
    Json(payload): Json<PluginPublisherReviewPayload>,
) -> Result<Json<PluginPublisherRecord>, ApiError> {
    ensure_super_admin(&user)?;
    let existing = state
        .store
        .get_plugin_publisher(record_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin publisher application not found"))?;
    let decision = normalize_publisher_decision(payload.decision.as_str())?;
    let review_note = normalize_review_note(payload.review_note.as_deref());
    let next_status = validate_publisher_review_transition(
        existing.status.as_str(),
        decision.as_str(),
        review_note.as_deref(),
    )?;

    let now = now_rfc3339();
    let mut updated = existing.clone();
    updated.status = next_status;
    updated.reviewed_at = Some(now.clone());
    updated.reviewed_by = Some(user.user_id.clone());
    updated.review_note = review_note;
    updated.updated_at = now;
    let replaced = state
        .store
        .replace_plugin_publisher_if_matches(&existing, &updated)
        .await
        .map_err(ApiError::internal)?;
    if !replaced {
        return Err(ApiError::conflict(
            "Publisher application changed concurrently; reload before reviewing",
        ));
    }
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_REVIEW_PUBLISHER,
        user.user_id.as_str(),
        None,
        updated.id.as_str(),
        None,
        "success",
        BTreeMap::from([
            ("publisher_id".to_string(), json!(updated.publisher_id)),
            ("marketplace_id".to_string(), json!(updated.marketplace_id)),
            ("decision".to_string(), json!(decision)),
            ("status".to_string(), json!(updated.status)),
            (
                "review_note_present".to_string(),
                json!(updated.review_note.is_some()),
            ),
        ]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(updated))
}

pub(super) async fn require_approved_publisher_identity(
    state: &AppState,
    marketplace: &PluginMarketplaceRecord,
    publisher: &PluginPublisher,
) -> Result<Option<PluginPublisherRecord>, ApiError> {
    if !marketplace_requires_approved_publisher(marketplace) {
        return Ok(None);
    }
    let record = state
        .store
        .find_plugin_publisher(marketplace.id.as_str(), publisher.id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::conflict("Plugin publisher is not approved"))?;
    if record.status != PLUGIN_PUBLISHER_STATUS_APPROVED {
        return Err(ApiError::conflict("Plugin publisher is not approved"));
    }
    if record.name != publisher.name || record.website != publisher.website {
        return Err(ApiError::conflict(
            "Plugin publisher metadata does not match the approved identity",
        ));
    }
    Ok(Some(record))
}

pub(super) async fn require_approved_publisher_release_key(
    state: &AppState,
    marketplace: &PluginMarketplaceRecord,
    publisher: &PluginPublisher,
    key_id: &str,
) -> Result<(), ApiError> {
    let Some(record) = require_approved_publisher_identity(state, marketplace, publisher).await?
    else {
        return Ok(());
    };
    if !record
        .signing_keys
        .iter()
        .any(|key| key.key_id == key_id && key.revoked_at.is_none())
    {
        return Err(ApiError::conflict(
            "Plugin release signing key is not approved for this publisher",
        ));
    }
    Ok(())
}

fn marketplace_requires_approved_publisher(marketplace: &PluginMarketplaceRecord) -> bool {
    marketplace.source_kind == PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY
        && marketplace.visibility == PLUGIN_VISIBILITY_PUBLIC
        && marketplace.owner_user_id.is_none()
}

fn ensure_publisher_onboarding_marketplace(
    marketplace: &PluginMarketplaceRecord,
) -> Result<(), ApiError> {
    if !marketplace.enabled
        || marketplace.source_kind != PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY
        || marketplace.trust_level != PLUGIN_TRUST_TRUSTED
        || marketplace.visibility != PLUGIN_VISIBILITY_PUBLIC
        || marketplace.owner_user_id.is_some()
    {
        return Err(ApiError::conflict(
            "Publisher onboarding requires an enabled public trusted admin_registry marketplace",
        ));
    }
    Ok(())
}

fn normalize_publisher_query(query: &mut PluginPublisherQuery) -> Result<(), ApiError> {
    if let Some(marketplace_id) = query.marketplace_id.take() {
        query.marketplace_id = Some(validate_plugin_identifier(
            marketplace_id.as_str(),
            "marketplace_id",
        )?);
    }
    if let Some(status) = query.status.take() {
        let status = status.trim().to_ascii_lowercase();
        if !is_publisher_status(status.as_str()) {
            return Err(ApiError::bad_request("invalid publisher status"));
        }
        query.status = Some(status);
    }
    Ok(())
}

fn normalize_publisher_application(
    payload: &mut PluginPublisherApplicationPayload,
) -> Result<(), ApiError> {
    payload.publisher_id =
        validate_plugin_identifier(payload.publisher_id.as_str(), "publisher_id")?;
    payload.marketplace_id =
        validate_plugin_identifier(payload.marketplace_id.as_str(), "marketplace_id")?;
    payload.name = required_text(Some(payload.name.as_str()), "name")?;
    payload.website = normalize_https_url(payload.website.as_deref(), "website")?;
    Ok(())
}

fn normalize_publisher_decision(value: &str) -> Result<String, ApiError> {
    let value = value.trim().to_ascii_lowercase();
    if matches!(
        value.as_str(),
        PLUGIN_PUBLISHER_DECISION_APPROVE
            | PLUGIN_PUBLISHER_DECISION_REJECT
            | PLUGIN_PUBLISHER_DECISION_SUSPEND
    ) {
        Ok(value)
    } else {
        Err(ApiError::bad_request(
            "decision must be approve, reject, or suspend",
        ))
    }
}

fn normalize_review_note(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(MAX_REVIEW_NOTE_CHARS).collect())
}

fn validate_publisher_review_transition(
    current_status: &str,
    decision: &str,
    review_note: Option<&str>,
) -> Result<String, ApiError> {
    if matches!(
        decision,
        PLUGIN_PUBLISHER_DECISION_REJECT | PLUGIN_PUBLISHER_DECISION_SUSPEND
    ) && review_note.is_none()
    {
        return Err(ApiError::bad_request(
            "review_note is required when rejecting or suspending a publisher",
        ));
    }
    let next = match (current_status, decision) {
        (PLUGIN_PUBLISHER_STATUS_PENDING, PLUGIN_PUBLISHER_DECISION_APPROVE)
        | (PLUGIN_PUBLISHER_STATUS_SUSPENDED, PLUGIN_PUBLISHER_DECISION_APPROVE) => {
            PLUGIN_PUBLISHER_STATUS_APPROVED
        }
        (PLUGIN_PUBLISHER_STATUS_PENDING, PLUGIN_PUBLISHER_DECISION_REJECT) => {
            PLUGIN_PUBLISHER_STATUS_REJECTED
        }
        (PLUGIN_PUBLISHER_STATUS_APPROVED, PLUGIN_PUBLISHER_DECISION_SUSPEND) => {
            PLUGIN_PUBLISHER_STATUS_SUSPENDED
        }
        _ => {
            return Err(ApiError::conflict(format!(
                "publisher review transition is not allowed: {current_status} -> {decision}"
            )));
        }
    };
    Ok(next.to_string())
}

fn is_publisher_status(value: &str) -> bool {
    matches!(
        value,
        PLUGIN_PUBLISHER_STATUS_PENDING
            | PLUGIN_PUBLISHER_STATUS_APPROVED
            | PLUGIN_PUBLISHER_STATUS_REJECTED
            | PLUGIN_PUBLISHER_STATUS_SUSPENDED
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_application_uses_platform_managed_signing() {
        let mut payload = test_application();
        assert!(normalize_publisher_application(&mut payload).is_ok());
    }

    #[test]
    fn publisher_review_state_machine_is_fail_closed() {
        assert_eq!(
            validate_publisher_review_transition(
                PLUGIN_PUBLISHER_STATUS_PENDING,
                PLUGIN_PUBLISHER_DECISION_APPROVE,
                None,
            )
            .expect("approve pending"),
            PLUGIN_PUBLISHER_STATUS_APPROVED
        );
        assert!(validate_publisher_review_transition(
            PLUGIN_PUBLISHER_STATUS_PENDING,
            PLUGIN_PUBLISHER_DECISION_REJECT,
            None,
        )
        .is_err());
        assert_eq!(
            validate_publisher_review_transition(
                PLUGIN_PUBLISHER_STATUS_APPROVED,
                PLUGIN_PUBLISHER_DECISION_SUSPEND,
                Some("policy violation"),
            )
            .expect("suspend approved"),
            PLUGIN_PUBLISHER_STATUS_SUSPENDED
        );
        assert!(validate_publisher_review_transition(
            PLUGIN_PUBLISHER_STATUS_REJECTED,
            PLUGIN_PUBLISHER_DECISION_APPROVE,
            None,
        )
        .is_err());
    }

    #[test]
    fn only_public_trusted_admin_marketplaces_accept_onboarding() {
        let valid = test_marketplace();
        assert!(ensure_publisher_onboarding_marketplace(&valid).is_ok());

        let mut private = valid.clone();
        private.visibility = PLUGIN_VISIBILITY_PRIVATE.to_string();
        private.owner_user_id = Some("user-1".to_string());
        assert!(ensure_publisher_onboarding_marketplace(&private).is_err());

        let mut disabled = valid;
        disabled.enabled = false;
        assert!(ensure_publisher_onboarding_marketplace(&disabled).is_err());
    }

    fn test_application() -> PluginPublisherApplicationPayload {
        PluginPublisherApplicationPayload {
            publisher_id: "publisher-demo".to_string(),
            marketplace_id: "marketplace-demo".to_string(),
            name: "Publisher Demo".to_string(),
            website: Some("https://publisher.example.com".to_string()),
        }
    }

    fn test_marketplace() -> PluginMarketplaceRecord {
        PluginMarketplaceRecord {
            id: "marketplace-demo".to_string(),
            name: "marketplace-demo".to_string(),
            owner_user_id: None,
            visibility: PLUGIN_VISIBILITY_PUBLIC.to_string(),
            source_kind: PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY.to_string(),
            catalog_url: Some("https://plugins.example.com/catalog.json".to_string()),
            enabled: true,
            trust_level: PLUGIN_TRUST_TRUSTED.to_string(),
            trusted_signing_keys: Vec::new(),
            last_catalog_revision: None,
            last_synced_at: None,
        }
    }
}
