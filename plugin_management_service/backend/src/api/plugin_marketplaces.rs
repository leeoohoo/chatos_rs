// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_plugin_management_sdk::{
    PLUGIN_SIGNATURE_ALGORITHM_ED25519, PLUGIN_SIGNING_KEY_USAGE_CATALOG,
    PLUGIN_SIGNING_KEY_USAGE_RELEASE,
};

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
    validate_marketplace_signing_keys(
        &trusted_signing_keys,
        trust_level == PLUGIN_TRUST_TRUSTED
            && source_kind != PLUGIN_MARKETPLACE_SOURCE_LOCAL_DIRECTORY,
    )?;

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
        .replace_plugin_marketplace(&record)
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
    Ok(Json(record))
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
        PLUGIN_MARKETPLACE_SOURCE_LOCAL_DIRECTORY => {
            Ok(PLUGIN_MARKETPLACE_SOURCE_LOCAL_DIRECTORY.to_string())
        }
        _ => Err(ApiError::bad_request(
            "source_kind must be official_registry, admin_registry, or local_directory",
        )),
    }
}

fn normalize_marketplace_trust(value: Option<&str>, source_kind: &str) -> Result<String, ApiError> {
    let trust = value
        .unwrap_or(
            if source_kind == PLUGIN_MARKETPLACE_SOURCE_LOCAL_DIRECTORY {
                PLUGIN_TRUST_UNTRUSTED
            } else {
                PLUGIN_TRUST_TRUSTED
            },
        )
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        trust.as_str(),
        PLUGIN_TRUST_BUNDLED | PLUGIN_TRUST_TRUSTED | PLUGIN_TRUST_UNTRUSTED
    ) {
        return Err(ApiError::bad_request(
            "trust_level must be bundled, trusted, or untrusted",
        ));
    }
    if source_kind == PLUGIN_MARKETPLACE_SOURCE_LOCAL_DIRECTORY && trust != PLUGIN_TRUST_UNTRUSTED {
        return Err(ApiError::bad_request(
            "local_directory marketplaces must remain untrusted",
        ));
    }
    Ok(trust)
}

fn validate_marketplace_signing_keys(
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
        has_catalog_root |= usages.contains(PLUGIN_SIGNING_KEY_USAGE_CATALOG);
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
    fn local_directory_marketplaces_cannot_claim_trusted_status() {
        assert!(normalize_marketplace_trust(
            Some(PLUGIN_TRUST_TRUSTED),
            PLUGIN_MARKETPLACE_SOURCE_LOCAL_DIRECTORY,
        )
        .is_err());
        assert_eq!(
            normalize_marketplace_trust(None, PLUGIN_MARKETPLACE_SOURCE_LOCAL_DIRECTORY,)
                .expect("default local trust"),
            PLUGIN_TRUST_UNTRUSTED
        );
    }

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
}
