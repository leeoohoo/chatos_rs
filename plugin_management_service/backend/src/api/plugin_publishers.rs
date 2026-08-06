// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use chatos_plugin_management_sdk::{PluginPublisher, PLUGIN_SIGNING_KEY_USAGE_RELEASE};

use super::plugin_catalog_sync::is_syncable_network_marketplace;
use super::plugin_marketplaces::{
    validate_marketplace_signing_key_progression, validate_marketplace_signing_keys,
};
use super::*;

const MAX_PUBLISHER_SIGNING_KEYS: usize = 32;
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
        signing_keys: payload.signing_keys,
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
            (
                "signing_key_ids".to_string(),
                json!(publisher_signing_key_ids(record.signing_keys.as_slice())),
            ),
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

    if decision == PLUGIN_PUBLISHER_DECISION_APPROVE {
        approve_publisher_signing_keys(&state, &existing).await?;
    }

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

async fn approve_publisher_signing_keys(
    state: &AppState,
    publisher: &PluginPublisherRecord,
) -> Result<(), ApiError> {
    let marketplace = state
        .store
        .get_plugin_marketplace(publisher.marketplace_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::conflict("Plugin marketplace not found"))?;
    ensure_publisher_onboarding_marketplace(&marketplace)?;
    let mut merged = marketplace.trusted_signing_keys.clone();
    for key in &publisher.signing_keys {
        if let Some(existing) = merged.iter().find(|item| item.key_id == key.key_id) {
            if existing != key {
                return Err(ApiError::conflict(format!(
                    "Marketplace signing key ID {} is already bound to different material",
                    key.key_id
                )));
            }
        } else {
            merged.push(key.clone());
        }
    }
    merged.sort_by(|left, right| left.key_id.cmp(&right.key_id));
    validate_marketplace_signing_keys(merged.as_slice(), true)?;
    validate_marketplace_signing_key_progression(
        marketplace.trusted_signing_keys.as_slice(),
        merged.as_slice(),
    )?;
    if merged == marketplace.trusted_signing_keys {
        return Ok(());
    }
    let mut updated = marketplace.clone();
    updated.trusted_signing_keys = merged;
    let replaced = state
        .store
        .replace_plugin_marketplace_if_matches_with_catalog_sync(
            &marketplace,
            &updated,
            is_syncable_network_marketplace(&updated),
        )
        .await
        .map_err(ApiError::internal)?;
    if !replaced {
        return Err(ApiError::conflict(
            "Plugin marketplace changed concurrently; reload before approving publisher",
        ));
    }
    if let Err(error) =
        crate::catalog_sync_queue::publish_pending_marketplace(state, updated.id.as_str()).await
    {
        tracing::warn!(
            marketplace_id = updated.id.as_str(),
            error = error.as_str(),
            "Plugin Management left Catalog sync event in Outbox after publisher approval"
        );
    }
    Ok(())
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
    if payload.signing_keys.is_empty() || payload.signing_keys.len() > MAX_PUBLISHER_SIGNING_KEYS {
        return Err(ApiError::bad_request(format!(
            "signing_keys must contain 1-{MAX_PUBLISHER_SIGNING_KEYS} keys"
        )));
    }
    validate_marketplace_signing_keys(payload.signing_keys.as_slice(), false)?;
    for key in &payload.signing_keys {
        if key.publisher_id != payload.publisher_id {
            return Err(ApiError::bad_request(
                "signing key publisher_id must match publisher_id",
            ));
        }
        if key.revoked_at.is_some()
            || key.usages.len() != 1
            || key.usages[0] != PLUGIN_SIGNING_KEY_USAGE_RELEASE
        {
            return Err(ApiError::bad_request(
                "publisher signing keys must be active release-only keys",
            ));
        }
    }
    payload
        .signing_keys
        .sort_by(|left, right| left.key_id.cmp(&right.key_id));
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

fn publisher_signing_key_ids(keys: &[SigningKeyRef]) -> Vec<String> {
    let mut ids = keys
        .iter()
        .map(|key| key.key_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use chatos_plugin_management_sdk::{
        PLUGIN_SIGNATURE_ALGORITHM_ED25519, PLUGIN_SIGNING_KEY_USAGE_CATALOG,
    };

    use super::*;

    #[test]
    fn publisher_application_requires_active_release_only_keys() {
        let mut payload = test_application();
        assert!(normalize_publisher_application(&mut payload).is_ok());

        let mut catalog_key = test_application();
        catalog_key.signing_keys[0].usages = vec![PLUGIN_SIGNING_KEY_USAGE_CATALOG.to_string()];
        assert!(normalize_publisher_application(&mut catalog_key).is_err());

        let mut wrong_publisher = test_application();
        wrong_publisher.signing_keys[0].publisher_id = "another-publisher".to_string();
        assert!(normalize_publisher_application(&mut wrong_publisher).is_err());
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
            signing_keys: vec![SigningKeyRef {
                key_id: "publisher-release-v1".to_string(),
                publisher_id: "publisher-demo".to_string(),
                algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
                public_key_base64: STANDARD.encode([7_u8; 32]),
                usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
                valid_from: "2026-01-01T00:00:00Z".to_string(),
                valid_until: Some("2027-01-01T00:00:00Z".to_string()),
                revoked_at: None,
            }],
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
