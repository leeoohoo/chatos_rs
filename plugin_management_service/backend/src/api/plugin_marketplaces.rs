// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_plugin_management_sdk::{
    PLUGIN_SIGNATURE_ALGORITHM_ED25519, PLUGIN_SIGNING_KEY_USAGE_CATALOG,
    PLUGIN_SIGNING_KEY_USAGE_RELEASE,
};
use serde_json::Value;

use super::plugin_catalog_sync::is_syncable_network_marketplace;
use super::*;

pub(super) async fn list_admin_plugin_marketplaces(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ListResponse<PluginMarketplaceRecord>>, ApiError> {
    ensure_super_admin(&user)?;
    list_plugin_marketplaces(State(state), Extension(user)).await
}

pub(super) async fn create_admin_plugin_marketplace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<PluginMarketplacePayload>,
) -> Result<Json<PluginMarketplaceRecord>, ApiError> {
    ensure_super_admin(&user)?;
    create_plugin_marketplace(State(state), Extension(user), Json(payload)).await
}

pub(super) async fn update_admin_plugin_marketplace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(marketplace_id): Path<String>,
    Json(payload): Json<PluginMarketplaceUpdatePayload>,
) -> Result<Json<PluginMarketplaceRecord>, ApiError> {
    ensure_super_admin(&user)?;
    let existing = state
        .store
        .get_plugin_marketplace(marketplace_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin marketplace not found"))?;
    let name = validate_plugin_identifier(payload.name.as_str(), "name")?;
    if name != existing.name
        && state
            .store
            .find_plugin_marketplace_by_name(name.as_str())
            .await
            .map_err(ApiError::internal)?
            .is_some()
    {
        return Err(ApiError::conflict("Plugin marketplace name already exists"));
    }
    let catalog_url = normalize_https_url(payload.catalog_url.as_deref(), "catalog_url")?;
    let trust_level = normalize_marketplace_trust(
        Some(payload.trust_level.as_str()),
        existing.source_kind.as_str(),
    )?;
    validate_marketplace_signing_keys(
        payload.trusted_signing_keys.as_slice(),
        trust_level == PLUGIN_TRUST_TRUSTED,
    )?;
    validate_marketplace_signing_key_progression(
        existing.trusted_signing_keys.as_slice(),
        payload.trusted_signing_keys.as_slice(),
    )?;

    let transition = signing_key_transition_summary(
        existing.trusted_signing_keys.as_slice(),
        payload.trusted_signing_keys.as_slice(),
    );
    let updated = PluginMarketplaceRecord {
        id: existing.id.clone(),
        name,
        owner_user_id: existing.owner_user_id.clone(),
        visibility: existing.visibility.clone(),
        source_kind: existing.source_kind.clone(),
        catalog_url,
        enabled: payload.enabled,
        trust_level,
        trusted_signing_keys: payload.trusted_signing_keys,
        last_catalog_revision: existing.last_catalog_revision.clone(),
        last_synced_at: existing.last_synced_at.clone(),
    };
    let replaced = state
        .store
        .replace_plugin_marketplace_if_matches_with_catalog_sync(
            &existing,
            &updated,
            is_syncable_network_marketplace(&updated),
        )
        .await
        .map_err(ApiError::internal)?;
    if !replaced {
        return Err(ApiError::conflict(
            "Plugin marketplace changed concurrently; reload before updating",
        ));
    }
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_UPDATE_MARKETPLACE,
        user.effective_owner_user_id(),
        None,
        format!("marketplace:{}", updated.id).as_str(),
        None,
        "success",
        BTreeMap::from([
            ("marketplace_id".to_string(), json!(updated.id)),
            ("enabled".to_string(), json!(updated.enabled)),
            ("trust_level".to_string(), json!(updated.trust_level)),
            (
                "catalog_url_changed".to_string(),
                json!(existing.catalog_url != updated.catalog_url),
            ),
            ("signing_key_transition".to_string(), transition),
        ]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    publish_catalog_sync_outbox(&state, updated.id.as_str()).await;
    Ok(Json(updated))
}

pub(super) async fn list_plugin_marketplaces(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ListResponse<PluginMarketplaceRecord>>, ApiError> {
    let mut items = state
        .store
        .list_plugin_marketplaces()
        .await
        .map_err(ApiError::internal)?;
    if !user.is_super_admin() {
        items.retain(|marketplace| ensure_marketplace_visible(&user, marketplace).is_ok());
    }
    Ok(Json(ListResponse {
        total: items.len() as u64,
        items,
    }))
}

pub(super) async fn create_plugin_marketplace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<PluginMarketplacePayload>,
) -> Result<Json<PluginMarketplaceRecord>, ApiError> {
    let personal = !user.is_super_admin();
    let name = validate_plugin_identifier(payload.name.as_deref().unwrap_or_default(), "name")?;
    let id = match (personal, payload.id.as_deref()) {
        (true, Some(_)) => {
            return Err(ApiError::forbidden(
                "personal Plugin Marketplace IDs are assigned by the service",
            ));
        }
        (true, None) => format!("personal-{}", Uuid::new_v4()),
        (false, Some(value)) => validate_plugin_identifier(value, "id")?,
        (false, None) => name.clone(),
    };
    let source_kind = normalize_marketplace_source(payload.source_kind.as_deref())?;
    let trust_level = normalize_marketplace_trust(payload.trust_level.as_deref(), &source_kind)?;
    let catalog_url = normalize_https_url(payload.catalog_url.as_deref(), "catalog_url")?;
    if personal {
        validate_personal_marketplace_configuration(
            source_kind.as_str(),
            trust_level.as_str(),
            catalog_url.as_deref(),
        )?;
    }
    let trusted_signing_keys = payload.trusted_signing_keys.unwrap_or_default();
    validate_marketplace_signing_keys(&trusted_signing_keys, trust_level == PLUGIN_TRUST_TRUSTED)?;

    if state
        .store
        .get_plugin_marketplace(id.as_str())
        .await
        .map_err(ApiError::internal)?
        .is_some()
        || state
            .store
            .find_plugin_marketplace_by_name(name.as_str())
            .await
            .map_err(ApiError::internal)?
            .is_some()
    {
        return Err(ApiError::conflict("Plugin marketplace already exists"));
    }

    let record = PluginMarketplaceRecord {
        id: id.clone(),
        name,
        owner_user_id: personal.then(|| user.effective_owner_user_id().to_string()),
        visibility: if personal {
            PLUGIN_VISIBILITY_PRIVATE.to_string()
        } else {
            PLUGIN_VISIBILITY_PUBLIC.to_string()
        },
        source_kind,
        catalog_url,
        enabled: payload.enabled.unwrap_or(true),
        trust_level,
        trusted_signing_keys,
        last_catalog_revision: None,
        last_synced_at: None,
    };
    state
        .store
        .replace_plugin_marketplace_with_catalog_sync(
            &record,
            is_syncable_network_marketplace(&record),
        )
        .await
        .map_err(ApiError::internal)?;
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_PUBLISH_MARKETPLACE,
        user.effective_owner_user_id(),
        None,
        format!("marketplace:{}", record.id).as_str(),
        None,
        "success",
        BTreeMap::from([
            ("marketplace_id".to_string(), json!(record.id)),
            ("visibility".to_string(), json!(record.visibility)),
            ("owner_user_id".to_string(), json!(record.owner_user_id)),
        ]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    publish_catalog_sync_outbox(&state, record.id.as_str()).await;
    Ok(Json(record))
}

async fn publish_catalog_sync_outbox(state: &AppState, marketplace_id: &str) {
    if let Err(error) =
        crate::catalog_sync_queue::publish_pending_marketplace(state, marketplace_id).await
    {
        tracing::warn!(
            marketplace_id,
            error = error.as_str(),
            "Plugin Management left Catalog sync event in Outbox"
        );
    }
}

fn validate_personal_marketplace_configuration(
    source_kind: &str,
    trust_level: &str,
    catalog_url: Option<&str>,
) -> Result<(), ApiError> {
    if source_kind != PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY {
        return Err(ApiError::forbidden(
            "personal Plugin Marketplaces must use admin_registry",
        ));
    }
    if trust_level != PLUGIN_TRUST_TRUSTED {
        return Err(ApiError::forbidden(
            "personal Plugin Marketplaces must use trusted signed Catalogs",
        ));
    }
    if catalog_url.is_none() {
        return Err(ApiError::bad_request(
            "personal Plugin Marketplaces require catalog_url",
        ));
    }
    Ok(())
}

fn normalize_marketplace_source(value: Option<&str>) -> Result<String, ApiError> {
    match value
        .unwrap_or(PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY)
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY => {
            Ok(PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY.to_string())
        }
        PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY => {
            Ok(PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY.to_string())
        }
        _ => Err(ApiError::bad_request(
            "source_kind must be official_registry or admin_registry",
        )),
    }
}

fn normalize_marketplace_trust(
    value: Option<&str>,
    _source_kind: &str,
) -> Result<String, ApiError> {
    let trust = value
        .unwrap_or(PLUGIN_TRUST_TRUSTED)
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        trust.as_str(),
        PLUGIN_TRUST_TRUSTED | PLUGIN_TRUST_UNTRUSTED
    ) {
        return Err(ApiError::bad_request(
            "trust_level must be trusted or untrusted",
        ));
    }
    Ok(trust)
}

pub(super) fn validate_marketplace_signing_keys(
    keys: &[SigningKeyRef],
    require_catalog_root: bool,
) -> Result<(), ApiError> {
    let mut key_ids = HashSet::new();
    let mut has_catalog_root = false;
    for key in keys {
        required_text(Some(key.key_id.as_str()), "trusted_signing_keys.key_id")?;
        required_text(
            Some(key.publisher_id.as_str()),
            "trusted_signing_keys.publisher_id",
        )?;
        required_text(
            Some(key.public_key_base64.as_str()),
            "trusted_signing_keys.public_key_base64",
        )?;
        required_text(
            Some(key.valid_from.as_str()),
            "trusted_signing_keys.valid_from",
        )?;
        if key.algorithm != PLUGIN_SIGNATURE_ALGORITHM_ED25519 {
            return Err(ApiError::bad_request(
                "trusted signing key algorithm must be ed25519",
            ));
        }
        if !key_ids.insert(key.key_id.as_str()) {
            return Err(ApiError::bad_request("duplicate trusted signing key id"));
        }
        let mut usages = HashSet::new();
        for usage in &key.usages {
            if !matches!(
                usage.as_str(),
                PLUGIN_SIGNING_KEY_USAGE_CATALOG | PLUGIN_SIGNING_KEY_USAGE_RELEASE
            ) {
                return Err(ApiError::bad_request(
                    "trusted signing key usage must be catalog or release",
                ));
            }
            if !usages.insert(usage.as_str()) {
                return Err(ApiError::bad_request("duplicate trusted signing key usage"));
            }
        }
        has_catalog_root |=
            key.revoked_at.is_none() && usages.contains(PLUGIN_SIGNING_KEY_USAGE_CATALOG);
        let public_key = STANDARD
            .decode(key.public_key_base64.as_bytes())
            .map_err(|_| ApiError::bad_request("trusted signing key must use valid base64"))?;
        if public_key.len() != 32 {
            return Err(ApiError::bad_request(
                "ed25519 trusted signing key must decode to 32 bytes",
            ));
        }
        let valid_from =
            parse_timestamp(key.valid_from.as_str(), "trusted_signing_keys.valid_from")?;
        if let Some(valid_until) = key.valid_until.as_deref() {
            let valid_until = parse_timestamp(valid_until, "trusted_signing_keys.valid_until")?;
            if valid_until <= valid_from {
                return Err(ApiError::bad_request(
                    "trusted signing key valid_until must be later than valid_from",
                ));
            }
        }
        if let Some(revoked_at) = key.revoked_at.as_deref() {
            parse_timestamp(revoked_at, "trusted_signing_keys.revoked_at")?;
        }
    }
    if require_catalog_root && !has_catalog_root {
        return Err(ApiError::bad_request(
            "trusted network Marketplace requires a signing key with catalog usage",
        ));
    }
    Ok(())
}

pub(super) fn validate_marketplace_signing_key_progression(
    previous: &[SigningKeyRef],
    next: &[SigningKeyRef],
) -> Result<(), ApiError> {
    let next_keys = next
        .iter()
        .map(|key| (key.key_id.as_str(), key))
        .collect::<HashMap<_, _>>();
    for key in previous {
        let Some(updated) = next_keys.get(key.key_id.as_str()) else {
            if key.revoked_at.is_some() {
                continue;
            }
            return Err(ApiError::conflict(format!(
                "Marketplace update removes non-revoked signing key {}",
                key.key_id
            )));
        };
        if key.publisher_id != updated.publisher_id
            || key.algorithm != updated.algorithm
            || key.public_key_base64 != updated.public_key_base64
            || key.usages.iter().collect::<BTreeSet<_>>()
                != updated.usages.iter().collect::<BTreeSet<_>>()
            || key.valid_from != updated.valid_from
        {
            return Err(ApiError::conflict(format!(
                "Marketplace update changes immutable signing key material {}",
                key.key_id
            )));
        }
        if key.revoked_at.is_some() && key.revoked_at != updated.revoked_at {
            return Err(ApiError::conflict(format!(
                "Marketplace update removes or changes signing key revocation {}",
                key.key_id
            )));
        }
        validate_marketplace_key_valid_until_progression(key, updated)?;
    }
    Ok(())
}

fn validate_marketplace_key_valid_until_progression(
    previous: &SigningKeyRef,
    next: &SigningKeyRef,
) -> Result<(), ApiError> {
    let Some(previous_until) = previous.valid_until.as_deref() else {
        return Ok(());
    };
    let Some(next_until) = next.valid_until.as_deref() else {
        return Err(ApiError::conflict(format!(
            "Marketplace update extends signing key validity {}",
            previous.key_id
        )));
    };
    let previous_until = parse_timestamp(previous_until, "previous signing key valid_until")?;
    let next_until = parse_timestamp(next_until, "next signing key valid_until")?;
    if next_until > previous_until {
        return Err(ApiError::conflict(format!(
            "Marketplace update extends signing key validity {}",
            previous.key_id
        )));
    }
    Ok(())
}

fn signing_key_transition_summary(previous: &[SigningKeyRef], next: &[SigningKeyRef]) -> Value {
    let previous_by_id = previous
        .iter()
        .map(|key| (key.key_id.as_str(), key))
        .collect::<HashMap<_, _>>();
    let next_by_id = next
        .iter()
        .map(|key| (key.key_id.as_str(), key))
        .collect::<HashMap<_, _>>();
    let mut added = next_by_id
        .keys()
        .filter(|key_id| !previous_by_id.contains_key(*key_id))
        .map(|key_id| (*key_id).to_string())
        .collect::<Vec<_>>();
    let mut revoked = next_by_id
        .iter()
        .filter(|(key_id, key)| {
            key.revoked_at.is_some()
                && previous_by_id
                    .get(*key_id)
                    .is_some_and(|previous| previous.revoked_at.is_none())
        })
        .map(|(key_id, _)| (*key_id).to_string())
        .collect::<Vec<_>>();
    let mut removed = previous_by_id
        .keys()
        .filter(|key_id| !next_by_id.contains_key(*key_id))
        .map(|key_id| (*key_id).to_string())
        .collect::<Vec<_>>();
    added.sort();
    revoked.sort();
    removed.sort();
    json!({
        "added_key_ids": added,
        "revoked_key_ids": revoked,
        "removed_key_ids": removed,
    })
}

fn parse_timestamp(
    value: &str,
    field: &str,
) -> Result<chrono::DateTime<chrono::FixedOffset>, ApiError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|_| ApiError::bad_request(format!("{field} must use RFC3339")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_network_marketplaces_require_an_explicit_catalog_root() {
        let release_only = SigningKeyRef {
            key_id: "release-key".to_string(),
            publisher_id: "publisher".to_string(),
            algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
            public_key_base64: STANDARD.encode([1_u8; 32]),
            usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
            valid_from: "2026-01-01T00:00:00Z".to_string(),
            valid_until: Some("2027-01-01T00:00:00Z".to_string()),
            revoked_at: None,
        };
        assert!(
            validate_marketplace_signing_keys(std::slice::from_ref(&release_only), false).is_ok()
        );
        assert!(validate_marketplace_signing_keys(&[release_only], true).is_err());

        let mut revoked_catalog = test_catalog_key("catalog-key", 2);
        revoked_catalog.revoked_at = Some("2026-06-01T00:00:00Z".to_string());
        assert!(validate_marketplace_signing_keys(&[revoked_catalog], true).is_err());
    }

    #[test]
    fn marketplace_key_rotation_requires_revocation_before_removal() {
        let current = test_catalog_key("catalog-v1", 1);
        let successor = test_catalog_key("catalog-v2", 2);

        assert!(validate_marketplace_signing_key_progression(
            std::slice::from_ref(&current),
            std::slice::from_ref(&successor),
        )
        .is_err());

        let mut revoked = current.clone();
        revoked.revoked_at = Some("2026-07-28T00:00:00Z".to_string());
        assert!(validate_marketplace_signing_key_progression(
            std::slice::from_ref(&current),
            &[revoked.clone(), successor.clone()],
        )
        .is_ok());
        assert!(validate_marketplace_signing_key_progression(
            &[revoked, successor.clone()],
            std::slice::from_ref(&successor),
        )
        .is_ok());
    }

    #[test]
    fn marketplace_key_material_and_revocation_are_immutable() {
        let current = test_catalog_key("catalog-v1", 1);
        let mut changed_material = current.clone();
        changed_material.public_key_base64 = STANDARD.encode([9_u8; 32]);
        assert!(validate_marketplace_signing_key_progression(
            std::slice::from_ref(&current),
            &[changed_material],
        )
        .is_err());

        let mut revoked = current.clone();
        revoked.revoked_at = Some("2026-07-28T00:00:00Z".to_string());
        assert!(validate_marketplace_signing_key_progression(
            std::slice::from_ref(&revoked),
            std::slice::from_ref(&current),
        )
        .is_err());

        let mut extended = current.clone();
        extended.valid_until = Some("2028-01-01T00:00:00Z".to_string());
        assert!(validate_marketplace_signing_key_progression(
            std::slice::from_ref(&current),
            &[extended],
        )
        .is_err());
    }

    #[test]
    fn marketplace_key_transition_summary_contains_only_key_ids() {
        let current = test_catalog_key("catalog-v1", 1);
        let mut revoked = current.clone();
        revoked.revoked_at = Some("2026-07-28T00:00:00Z".to_string());
        let successor = test_catalog_key("catalog-v2", 2);
        assert_eq!(
            signing_key_transition_summary(&[current], &[revoked, successor]),
            json!({
                "added_key_ids": ["catalog-v2"],
                "revoked_key_ids": ["catalog-v1"],
                "removed_key_ids": [],
            })
        );
    }

    #[test]
    fn personal_marketplaces_are_private_network_registries() {
        assert!(validate_personal_marketplace_configuration(
            PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY,
            PLUGIN_TRUST_TRUSTED,
            Some("https://plugins.example.com/catalog.json"),
        )
        .is_ok());
        assert!(validate_personal_marketplace_configuration(
            PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY,
            PLUGIN_TRUST_TRUSTED,
            Some("https://plugins.example.com/catalog.json"),
        )
        .is_err());
        assert!(validate_personal_marketplace_configuration(
            PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY,
            PLUGIN_TRUST_UNTRUSTED,
            Some("https://plugins.example.com/catalog.json"),
        )
        .is_err());
        assert!(validate_personal_marketplace_configuration(
            PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY,
            PLUGIN_TRUST_TRUSTED,
            None,
        )
        .is_err());
    }

    fn test_catalog_key(key_id: &str, byte: u8) -> SigningKeyRef {
        SigningKeyRef {
            key_id: key_id.to_string(),
            publisher_id: "marketplace-authority".to_string(),
            algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
            public_key_base64: STANDARD.encode([byte; 32]),
            usages: vec![PLUGIN_SIGNING_KEY_USAGE_CATALOG.to_string()],
            valid_from: "2026-01-01T00:00:00Z".to_string(),
            valid_until: Some("2027-01-01T00:00:00Z".to_string()),
            revoked_at: None,
        }
    }
}
