// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use reqwest::Url;

use super::*;

pub(super) fn validate_plugin_identifier(value: &str, field: &str) -> Result<String, ApiError> {
    let normalized = required_text(Some(value), field)?;
    let valid = normalized.len() <= 64
        && !normalized.starts_with('-')
        && !normalized.ends_with('-')
        && !normalized.contains("--")
        && normalized
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if valid {
        Ok(normalized)
    } else {
        Err(ApiError::bad_request(format!(
            "{field} must use lower-case kebab-case and be at most 64 characters"
        )))
    }
}

pub(super) fn normalize_sha256(value: &str, field: &str) -> Result<String, ApiError> {
    let normalized = required_text(Some(value), field)?
        .strip_prefix("sha256:")
        .unwrap_or(value.trim())
        .to_ascii_lowercase();
    if normalized.len() == 64 && normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(normalized)
    } else {
        Err(ApiError::bad_request(format!(
            "{field} must be a 64-character SHA-256 hex digest"
        )))
    }
}

pub(super) fn normalize_https_url(
    value: Option<&str>,
    field: &str,
) -> Result<Option<String>, ApiError> {
    let Some(value) = normalized(value) else {
        return Ok(None);
    };
    let valid = Url::parse(value.as_str())
        .ok()
        .is_some_and(|url| url.scheme() == "https" && url.host_str().is_some());
    if valid {
        Ok(Some(value))
    } else {
        Err(ApiError::bad_request(format!(
            "{field} must be an absolute https:// URL"
        )))
    }
}

pub(super) fn normalize_release_channel(value: &str) -> Result<String, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stable" => Ok("stable".to_string()),
        "beta" => Ok("beta".to_string()),
        "canary" => Ok("canary".to_string()),
        _ => Err(ApiError::bad_request(
            "release_channel must be stable, beta, or canary",
        )),
    }
}

pub(super) fn normalize_plugin_visibility(value: &str) -> Result<String, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        PLUGIN_VISIBILITY_PUBLIC => Ok(PLUGIN_VISIBILITY_PUBLIC.to_string()),
        PLUGIN_VISIBILITY_PRIVATE => Ok(PLUGIN_VISIBILITY_PRIVATE.to_string()),
        _ => Err(ApiError::bad_request(
            "plugin visibility must be public or private",
        )),
    }
}

pub(super) fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

pub(super) fn validate_component_selection(
    selected: &[String],
    release: Option<&PluginReleaseRecord>,
) -> Result<(), ApiError> {
    if selected.is_empty() {
        return Ok(());
    }
    let release = release.ok_or_else(|| {
        ApiError::conflict("plugin has no published release for component selection")
    })?;
    let known = release
        .components
        .iter()
        .map(|component| component.component_key.as_str())
        .collect::<HashSet<_>>();
    if let Some(unknown) = selected.iter().find(|item| !known.contains(item.as_str())) {
        return Err(ApiError::bad_request(format!(
            "unknown plugin component: {unknown}"
        )));
    }
    Ok(())
}

pub(super) fn ensure_catalog_visible(
    user: &CurrentUser,
    plugin: &PluginCatalogRecord,
) -> Result<(), ApiError> {
    if user.is_super_admin()
        || (plugin.enabled
            && (plugin.visibility == PLUGIN_VISIBILITY_PUBLIC
                || (plugin.visibility == PLUGIN_VISIBILITY_PRIVATE
                    && plugin.owner_user_id.as_deref() == Some(user.effective_owner_user_id()))))
    {
        Ok(())
    } else {
        Err(ApiError::not_found("Plugin not found"))
    }
}

pub(super) fn ensure_marketplace_visible(
    user: &CurrentUser,
    marketplace: &PluginMarketplaceRecord,
) -> Result<(), ApiError> {
    if user.is_super_admin()
        || marketplace.visibility == PLUGIN_VISIBILITY_PUBLIC
        || (marketplace.visibility == PLUGIN_VISIBILITY_PRIVATE
            && marketplace.owner_user_id.as_deref() == Some(user.effective_owner_user_id()))
    {
        Ok(())
    } else {
        Err(ApiError::not_found("Plugin Marketplace not found"))
    }
}

pub(super) fn ensure_marketplace_writable(
    user: &CurrentUser,
    marketplace: &PluginMarketplaceRecord,
) -> Result<(), ApiError> {
    if user.is_super_admin()
        || (marketplace.visibility == PLUGIN_VISIBILITY_PRIVATE
            && marketplace.owner_user_id.as_deref() == Some(user.effective_owner_user_id()))
    {
        Ok(())
    } else {
        Err(ApiError::forbidden("Plugin Marketplace is not writable"))
    }
}

pub(super) fn apply_marketplace_catalog_scope(
    marketplace: &PluginMarketplaceRecord,
    plugin: &mut PluginCatalogRecord,
) {
    if marketplace.visibility == PLUGIN_VISIBILITY_PRIVATE {
        plugin.visibility = PLUGIN_VISIBILITY_PRIVATE.to_string();
        plugin.owner_user_id = marketplace.owner_user_id.clone();
    } else {
        plugin.owner_user_id = None;
    }
}

pub(super) fn ensure_catalog_visible_to_owner(
    owner_user_id: &str,
    plugin: &PluginCatalogRecord,
) -> Result<(), ApiError> {
    if plugin.enabled
        && (plugin.visibility == PLUGIN_VISIBILITY_PUBLIC
            || (plugin.visibility == PLUGIN_VISIBILITY_PRIVATE
                && plugin.owner_user_id.as_deref() == Some(owner_user_id)))
    {
        Ok(())
    } else {
        Err(ApiError::not_found("Plugin install source not found"))
    }
}

pub(super) fn plugin_audit_record(
    event: &str,
    owner_user_id: &str,
    device_id: Option<&str>,
    plugin_id: &str,
    release_id: Option<&str>,
    outcome: &str,
    details: BTreeMap<String, serde_json::Value>,
) -> PluginAuditLogRecord {
    PluginAuditLogRecord {
        id: Uuid::new_v4().to_string(),
        event: event.to_string(),
        owner_user_id: owner_user_id.to_string(),
        device_id: device_id.map(ToOwned::to_owned),
        plugin_id: plugin_id.to_string(),
        release_id: release_id.map(ToOwned::to_owned),
        component_key: None,
        outcome: outcome.to_string(),
        details,
        created_at: now_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user(user_id: &str, role: &str) -> CurrentUser {
        CurrentUser {
            principal_type: "human_user".to_string(),
            user_id: user_id.to_string(),
            username: user_id.to_string(),
            display_name: user_id.to_string(),
            role: role.to_string(),
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
        }
    }

    fn test_marketplace(owner_user_id: Option<&str>, visibility: &str) -> PluginMarketplaceRecord {
        PluginMarketplaceRecord {
            id: "marketplace".to_string(),
            name: "marketplace".to_string(),
            owner_user_id: owner_user_id.map(ToOwned::to_owned),
            visibility: visibility.to_string(),
            source_kind: PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY.to_string(),
            catalog_url: Some("https://plugins.example.com/catalog.json".to_string()),
            enabled: true,
            trust_level: PLUGIN_TRUST_TRUSTED.to_string(),
            trusted_signing_keys: Vec::new(),
            last_catalog_revision: None,
            last_synced_at: None,
        }
    }

    #[test]
    fn plugin_identifiers_are_stable_kebab_case() {
        assert_eq!(
            validate_plugin_identifier("documents", "name").expect("valid name"),
            "documents"
        );
        assert!(validate_plugin_identifier("Bad Name", "name").is_err());
        assert!(validate_plugin_identifier("bad--name", "name").is_err());
    }

    #[test]
    fn sha256_normalization_accepts_optional_prefix() {
        let digest = "a".repeat(64);
        assert_eq!(
            normalize_sha256(format!("sha256:{digest}").as_str(), "artifact_sha256")
                .expect("valid hash"),
            digest
        );
        assert!(normalize_sha256("bad", "artifact_sha256").is_err());
    }

    #[test]
    fn private_marketplaces_are_visible_and_writable_only_by_owner_or_admin() {
        let marketplace = test_marketplace(Some("owner-1"), PLUGIN_VISIBILITY_PRIVATE);
        let owner = test_user("owner-1", USER_ROLE_USER);
        let other = test_user("owner-2", USER_ROLE_USER);
        let admin = test_user("admin", USER_ROLE_SUPER_ADMIN);

        assert!(ensure_marketplace_visible(&owner, &marketplace).is_ok());
        assert!(ensure_marketplace_writable(&owner, &marketplace).is_ok());
        assert!(ensure_marketplace_visible(&other, &marketplace).is_err());
        assert!(ensure_marketplace_writable(&other, &marketplace).is_err());
        assert!(ensure_marketplace_visible(&admin, &marketplace).is_ok());
        assert!(ensure_marketplace_writable(&admin, &marketplace).is_ok());
    }

    #[test]
    fn public_marketplaces_are_readable_but_not_user_writable() {
        let marketplace = test_marketplace(None, PLUGIN_VISIBILITY_PUBLIC);
        let user = test_user("owner-1", USER_ROLE_USER);

        assert!(ensure_marketplace_visible(&user, &marketplace).is_ok());
        assert!(ensure_marketplace_writable(&user, &marketplace).is_err());
    }
}
