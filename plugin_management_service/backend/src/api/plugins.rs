// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use chatos_plugin_management_sdk::normalize_plugin_relative_path;

use super::plugin_publishers::require_approved_publisher_identity;
use super::*;

pub(super) async fn list_plugin_catalog(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<PluginCatalogQuery>,
) -> Result<Json<ListResponse<PluginCatalogRecord>>, ApiError> {
    state
        .store
        .list_plugin_catalog(
            &query,
            (!user.is_super_admin()).then(|| user.effective_owner_user_id()),
        )
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn list_admin_plugins(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<PluginCatalogQuery>,
) -> Result<Json<ListResponse<PluginCatalogListItem>>, ApiError> {
    ensure_super_admin(&user)?;
    let page = state
        .store
        .list_plugin_catalog(&query, None)
        .await
        .map_err(ApiError::internal)?;
    let release_ids: Vec<String> = page
        .items
        .iter()
        .map(|plugin| plugin.latest_release_id.trim())
        .filter(|release_id| !release_id.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    let releases_by_id: BTreeMap<String, PluginReleaseRecord> = state
        .store
        .list_plugin_releases_by_ids(&release_ids)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|release| (release.id.clone(), release))
        .collect();
    let items = page
        .items
        .into_iter()
        .map(|catalog| {
            let runtime_targets = releases_by_id
                .get(catalog.latest_release_id.as_str())
                .map(plugin_release_runtime_targets)
                .unwrap_or_default();
            PluginCatalogListItem {
                catalog,
                runtime_targets,
            }
        })
        .collect();
    Ok(Json(ListResponse {
        items,
        total: page.total,
    }))
}

pub(super) async fn get_plugin_catalog_entry(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(plugin_id): Path<String>,
) -> Result<Json<PluginCatalogRecord>, ApiError> {
    let plugin = state
        .store
        .get_plugin_catalog_entry(plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin not found"))?;
    ensure_catalog_visible(&user, &plugin)?;
    Ok(Json(plugin))
}

pub(super) async fn create_plugin_catalog_entry(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(mut payload): Json<PluginCatalogPayload>,
) -> Result<Json<PluginCatalogRecord>, ApiError> {
    ensure_super_admin(&user)?;
    normalize_catalog_payload(&mut payload)?;
    let marketplace = state
        .store
        .get_plugin_marketplace(payload.marketplace_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("Plugin marketplace not found"))?;
    if !marketplace.enabled {
        return Err(ApiError::conflict("Plugin marketplace is disabled"));
    }
    if require_approved_publisher_identity(&state, &marketplace, &payload.publisher)
        .await?
        .is_some()
    {
        payload.publisher.verified = true;
    }
    if state
        .store
        .find_plugin_catalog_entry(payload.marketplace_id.as_str(), payload.name.as_str())
        .await
        .map_err(ApiError::internal)?
        .is_some()
    {
        return Err(ApiError::conflict("Plugin already exists in marketplace"));
    }

    let now = now_rfc3339();
    let mut record = PluginCatalogRecord {
        id: Uuid::new_v4().to_string(),
        plugin_key: format!("{}@{}", payload.name, payload.marketplace_id),
        marketplace_id: payload.marketplace_id,
        owner_user_id: None,
        name: payload.name,
        display_name: payload.display_name,
        description: payload.description,
        publisher: payload.publisher,
        interface: payload.interface,
        keywords: payload.keywords,
        visibility: payload.visibility,
        featured: payload.featured,
        enabled: payload.enabled,
        latest_release_id: String::new(),
        license: payload.license,
        created_at: now.clone(),
        updated_at: now,
    };
    apply_marketplace_catalog_scope(&marketplace, &mut record);
    state
        .store
        .replace_plugin_catalog_entry(&record)
        .await
        .map_err(ApiError::internal)?;
    let mut details = BTreeMap::new();
    details.insert("plugin_key".to_string(), json!(record.plugin_key));
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_PUBLISH_CATALOG,
        user.user_id.as_str(),
        None,
        record.id.as_str(),
        None,
        "success",
        details,
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn update_user_plugin_preference(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(plugin_id): Path<String>,
    Json(payload): Json<UpdateUserPluginPreferencePayload>,
) -> Result<Json<UserPluginPreferenceRecord>, ApiError> {
    let plugin = state
        .store
        .get_plugin_catalog_entry(plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin not found"))?;
    ensure_catalog_visible(&user, &plugin)?;
    let owner_user_id = user.effective_owner_user_id().to_string();
    persist_user_plugin_preference(&state, owner_user_id.as_str(), &plugin, payload)
        .await
        .map(|response| Json(response.preference))
}

pub(super) async fn update_user_plugin_preference_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(plugin_id): Path<String>,
    Json(request): Json<UpdateUserPluginPreferenceRequest>,
) -> Result<Json<UpdateUserPluginPreferenceResponse>, ApiError> {
    let identity =
        require_local_connector_internal_request(&state, &headers, PLUGIN_INSTALL_MANAGE_SCOPE)?;
    let owner_user_id = required_text(Some(request.owner_user_id.as_str()), "owner_user_id")?;
    let mut internal_audit = PluginManagementInternalAuditGuard::new(
        &identity,
        Some(owner_user_id.as_str()),
        "user_plugin_preference",
        plugin_id.as_str(),
        "update",
    );
    let plugin = state
        .store
        .get_plugin_catalog_entry(plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin not found"))?;
    internal_audit.resource_name(Some(plugin.display_name.as_str()));
    ensure_catalog_visible_to_owner(owner_user_id.as_str(), &plugin)?;
    let response = persist_user_plugin_preference(
        &state,
        owner_user_id.as_str(),
        &plugin,
        UpdateUserPluginPreferencePayload {
            enabled: request.enabled,
            auto_update: request.auto_update,
            release_channel: request.release_channel,
            enabled_components: request.enabled_components,
        },
    )
    .await
    .map(Json)?;
    internal_audit.succeeded();
    Ok(response)
}

async fn persist_user_plugin_preference(
    state: &AppState,
    owner_user_id: &str,
    plugin: &PluginCatalogRecord,
    payload: UpdateUserPluginPreferencePayload,
) -> Result<UpdateUserPluginPreferenceResponse, ApiError> {
    let existing = state
        .store
        .get_user_plugin_preference(owner_user_id, plugin.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let enabled_components =
        normalize_string_list(payload.enabled_components.unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|record| record.enabled_components.clone())
                .unwrap_or_default()
        }));
    let latest_release = if plugin.latest_release_id.is_empty() {
        None
    } else {
        state
            .store
            .get_plugin_release(plugin.latest_release_id.as_str())
            .await
            .map_err(ApiError::internal)?
    };
    validate_component_selection(&enabled_components, latest_release.as_ref())?;
    let release_channel = normalize_release_channel(
        payload
            .release_channel
            .as_deref()
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|record| record.release_channel.as_str())
            })
            .unwrap_or("stable"),
    )?;
    let record = UserPluginPreferenceRecord {
        owner_user_id: owner_user_id.to_string(),
        plugin_id: plugin.id.clone(),
        enabled: payload.enabled,
        auto_update: payload
            .auto_update
            .or_else(|| existing.as_ref().map(|record| record.auto_update))
            .unwrap_or(false),
        release_channel,
        enabled_components,
        updated_at: now_rfc3339(),
    };
    let previous_enabled = existing.as_ref().map(|record| record.enabled);
    let disabled_transition = is_disabled_transition(previous_enabled, record.enabled);
    state
        .store
        .replace_user_plugin_preference(&record)
        .await
        .map_err(ApiError::internal)?;
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_UPDATE_PREFERENCE,
        owner_user_id,
        None,
        plugin.id.as_str(),
        latest_release.as_ref().map(|release| release.id.as_str()),
        "success",
        BTreeMap::from([
            ("enabled".to_string(), json!(record.enabled)),
            ("previous_enabled".to_string(), json!(previous_enabled)),
            (
                "disabled_transition".to_string(),
                json!(disabled_transition),
            ),
        ]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    Ok(UpdateUserPluginPreferenceResponse {
        preference: record,
        previous_enabled,
        disabled_transition,
    })
}

fn is_disabled_transition(previous_enabled: Option<bool>, enabled: bool) -> bool {
    previous_enabled == Some(true) && !enabled
}

fn plugin_release_runtime_targets(release: &PluginReleaseRecord) -> Vec<String> {
    plugin_execution_hosts_runtime_targets(
        release
            .components
            .iter()
            .map(|component| component.execution_host),
    )
}

fn plugin_execution_hosts_runtime_targets(
    hosts: impl IntoIterator<Item = PluginExecutionHost>,
) -> Vec<String> {
    let mut targets = BTreeSet::new();
    for host in hosts {
        match host {
            PluginExecutionHost::Local | PluginExecutionHost::Portable => {
                targets.insert(PLUGIN_RUNTIME_TARGET_LOCAL_CONNECTOR.to_string());
            }
        }
    }
    targets.into_iter().collect()
}

fn normalize_catalog_payload(payload: &mut PluginCatalogPayload) -> Result<(), ApiError> {
    payload.marketplace_id =
        validate_plugin_identifier(payload.marketplace_id.as_str(), "marketplace_id")?;
    payload.name = validate_plugin_identifier(payload.name.as_str(), "name")?;
    payload.display_name = required_text(Some(payload.display_name.as_str()), "display_name")?;
    payload.description = required_text(Some(payload.description.as_str()), "description")?;
    payload.visibility = normalize_plugin_visibility(payload.visibility.as_str())?;
    payload.keywords = normalize_string_list(std::mem::take(&mut payload.keywords));
    payload.publisher.id = required_text(Some(payload.publisher.id.as_str()), "publisher.id")?;
    payload.publisher.name =
        required_text(Some(payload.publisher.name.as_str()), "publisher.name")?;
    payload.publisher.website =
        normalize_https_url(payload.publisher.website.as_deref(), "publisher.website")?;
    payload.license.license_id = required_text(
        Some(payload.license.license_id.as_str()),
        "license.license_id",
    )?;
    payload.license.license_url = normalize_https_url(
        payload.license.license_url.as_deref(),
        "license.license_url",
    )?;
    if payload.license.redistributable && payload.license.reviewed_at.is_none() {
        return Err(ApiError::bad_request(
            "redistributable plugins require license.reviewed_at",
        ));
    }
    normalize_interface(&mut payload.interface)?;
    if payload.interface.display_name != payload.display_name {
        return Err(ApiError::bad_request(
            "interface.displayName must match display_name",
        ));
    }
    Ok(())
}

fn normalize_interface(interface: &mut PluginInterfaceMetadata) -> Result<(), ApiError> {
    interface.display_name = required_text(
        Some(interface.display_name.as_str()),
        "interface.displayName",
    )?;
    interface.short_description = required_text(
        Some(interface.short_description.as_str()),
        "interface.shortDescription",
    )?;
    interface.long_description = required_text(
        Some(interface.long_description.as_str()),
        "interface.longDescription",
    )?;
    interface.developer_name = required_text(
        Some(interface.developer_name.as_str()),
        "interface.developerName",
    )?;
    interface.category = required_text(Some(interface.category.as_str()), "interface.category")?;
    interface.capabilities = normalize_string_list(std::mem::take(&mut interface.capabilities));
    interface.default_prompt = std::mem::take(&mut interface.default_prompt)
        .into_iter()
        .map(|value| value.trim().chars().take(128).collect::<String>())
        .filter(|value| !value.is_empty())
        .take(3)
        .collect();
    if let Some(brand_color) = interface.brand_color.as_deref() {
        let valid = brand_color.len() == 7
            && brand_color.starts_with('#')
            && brand_color[1..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit());
        if !valid {
            return Err(ApiError::bad_request(
                "interface.brandColor must use #RRGGBB",
            ));
        }
    }
    interface.website_url =
        normalize_https_url(interface.website_url.as_deref(), "interface.websiteURL")?;
    interface.privacy_policy_url = normalize_https_url(
        interface.privacy_policy_url.as_deref(),
        "interface.privacyPolicyURL",
    )?;
    interface.terms_of_service_url = normalize_https_url(
        interface.terms_of_service_url.as_deref(),
        "interface.termsOfServiceURL",
    )?;
    for (field, path) in [
        ("interface.composerIcon", interface.composer_icon.as_mut()),
        ("interface.logo", interface.logo.as_mut()),
        ("interface.logoDark", interface.logo_dark.as_mut()),
    ] {
        if let Some(path) = path {
            path.path = normalize_plugin_relative_path(path.path.as_str())
                .map_err(|message| ApiError::bad_request(format!("{field}: {message}")))?;
            if !path.path.starts_with("./assets/") {
                return Err(ApiError::bad_request(format!(
                    "{field} must be stored under ./assets/"
                )));
            }
        }
    }
    for (index, path) in interface.screenshots.iter_mut().enumerate() {
        path.path = normalize_plugin_relative_path(path.path.as_str()).map_err(|message| {
            ApiError::bad_request(format!("interface.screenshots[{index}]: {message}"))
        })?;
        if !path.path.starts_with("./assets/") || !path.path.to_ascii_lowercase().ends_with(".png")
        {
            return Err(ApiError::bad_request(format!(
                "interface.screenshots[{index}] must be a PNG under ./assets/"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_license_requires_review_before_redistribution() {
        let mut payload = sample_payload();
        payload.license.redistributable = true;
        assert!(normalize_catalog_payload(&mut payload).is_err());
        payload.license.reviewed_at = Some("2026-07-22T00:00:00Z".to_string());
        assert!(normalize_catalog_payload(&mut payload).is_ok());
    }

    #[test]
    fn disable_transition_requires_an_authoritative_true_to_false_change() {
        assert!(is_disabled_transition(Some(true), false));
        assert!(!is_disabled_transition(Some(false), false));
        assert!(!is_disabled_transition(None, false));
        assert!(!is_disabled_transition(Some(true), true));
    }

    #[test]
    fn release_runtime_targets_expose_only_local_connector_execution() {
        assert_eq!(
            plugin_execution_hosts_runtime_targets([
                PluginExecutionHost::Portable,
                PluginExecutionHost::Local,
                PluginExecutionHost::Cloud,
            ]),
            vec![PLUGIN_RUNTIME_TARGET_LOCAL_CONNECTOR.to_string()],
        );
        assert_eq!(
            plugin_execution_hosts_runtime_targets([PluginExecutionHost::Cloud]),
            Vec::<String>::new(),
        );
        assert_eq!(
            plugin_execution_hosts_runtime_targets([PluginExecutionHost::Local]),
            vec![PLUGIN_RUNTIME_TARGET_LOCAL_CONNECTOR.to_string()],
        );
    }

    fn sample_payload() -> PluginCatalogPayload {
        PluginCatalogPayload {
            marketplace_id: "chatos-official".to_string(),
            name: "documents".to_string(),
            display_name: "Documents".to_string(),
            description: "Document workflows".to_string(),
            publisher: PluginPublisher {
                id: "chatos".to_string(),
                name: "ChatOS".to_string(),
                website: Some("https://example.com".to_string()),
                verified: true,
            },
            interface: PluginInterfaceMetadata {
                display_name: "Documents".to_string(),
                short_description: "Create documents".to_string(),
                long_description: "Create and edit documents".to_string(),
                developer_name: "ChatOS".to_string(),
                category: "Productivity".to_string(),
                capabilities: vec!["Write".to_string()],
                website_url: Some("https://example.com".to_string()),
                privacy_policy_url: None,
                terms_of_service_url: None,
                default_prompt: Vec::new(),
                brand_color: None,
                composer_icon: None,
                logo: None,
                logo_dark: None,
                screenshots: Vec::new(),
            },
            keywords: vec!["documents".to_string()],
            visibility: PLUGIN_VISIBILITY_PUBLIC.to_string(),
            featured: false,
            enabled: true,
            license: PluginLicenseMetadata {
                license_id: "MIT".to_string(),
                license_url: None,
                redistributable: false,
                reviewed_at: None,
            },
        }
    }
}
