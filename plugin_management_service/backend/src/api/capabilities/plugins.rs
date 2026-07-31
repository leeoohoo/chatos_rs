// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use super::*;

pub(super) struct PluginComponentGate {
    pub available: bool,
    pub reason: Option<String>,
}

pub(super) async fn availability_for_mcp_with_plugin_gate(
    state: &AppState,
    resource: &McpRecord,
    owner_user_id: &str,
    device_id: Option<&str>,
    runtime_provider: Option<&str>,
) -> Result<(bool, String, Option<String>), ApiError> {
    match plugin_component_gate(
        state,
        &resource.plugin_component,
        owner_user_id,
        device_id,
        runtime_provider,
    )
    .await?
    {
        Some(gate) if !gate.available => Ok((false, "plugin_unavailable".to_string(), gate.reason)),
        _ => super::availability_for_mcp(state, resource).await,
    }
}

pub(super) async fn availability_for_skill_with_plugin_gate(
    state: &AppState,
    resource: &SkillRecord,
    owner_user_id: &str,
    device_id: Option<&str>,
    runtime_provider: Option<&str>,
) -> Result<
    (
        bool,
        String,
        Option<String>,
        Option<SkillInstallationRecord>,
    ),
    ApiError,
> {
    match plugin_component_gate(
        state,
        &resource.plugin_component,
        owner_user_id,
        device_id,
        runtime_provider,
    )
    .await?
    {
        Some(gate) if !gate.available => {
            Ok((false, "plugin_unavailable".to_string(), gate.reason, None))
        }
        _ => super::availability_for_skill(state, resource, owner_user_id).await,
    }
}

pub(super) async fn resolve_plugin_binding(
    state: &AppState,
    binding: AgentBindingRecord,
    owner_user_id: &str,
    device_id: Option<&str>,
    runtime_provider: Option<&str>,
) -> Result<Option<ResolvedPlugin>, ApiError> {
    let Some(catalog) = state
        .store
        .get_plugin_catalog_entry(binding.resource_id.as_str())
        .await
        .map_err(ApiError::internal)?
    else {
        if binding.required {
            return Err(ApiError::conflict(format!(
                "required Plugin binding references a missing Plugin: {}",
                binding.resource_id
            )));
        }
        return Ok(None);
    };
    let preference = state
        .store
        .get_user_plugin_preference(owner_user_id, catalog.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let installation = match device_id {
        Some(device_id) => state
            .store
            .get_plugin_installation(owner_user_id, device_id, catalog.id.as_str())
            .await
            .map_err(ApiError::internal)?,
        None => None,
    };
    let release = match installation.as_ref() {
        Some(installation) => state
            .store
            .get_plugin_release(installation.release_id.as_str())
            .await
            .map_err(ApiError::internal)?,
        None if !catalog.latest_release_id.is_empty() => state
            .store
            .get_plugin_release(catalog.latest_release_id.as_str())
            .await
            .map_err(ApiError::internal)?,
        None => None,
    };
    let component_snapshots = match release.as_ref() {
        Some(release) => state
            .store
            .list_plugin_component_snapshots(catalog.id.as_str(), release.id.as_str())
            .await
            .map_err(ApiError::internal)?,
        None => Vec::new(),
    };
    let mut cloud_bundle_keys = match release.as_ref() {
        Some(release) => state
            .store
            .list_plugin_cloud_component_bundles(catalog.id.as_str(), release.id.as_str())
            .await
            .map_err(ApiError::internal)?
            .into_iter()
            .map(|bundle| bundle.component_key)
            .collect::<HashSet<_>>(),
        None => HashSet::new(),
    };
    if let Some(release) = release.as_ref() {
        let runtime_bundles = state
            .store
            .list_plugin_mcp_cloud_runtime_bundles(catalog.id.as_str(), release.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        for bundle in runtime_bundles {
            if component_snapshots.iter().any(|snapshot| {
                snapshot.plugin_id == bundle.plugin_id
                    && snapshot.release_id == bundle.release_id
                    && snapshot.component == bundle.component
                    && snapshot.content_sha256 == bundle.bundle_sha256
                    && chatos_plugin_management_sdk::plugin_mcp_cloud_runtime_bundle_sha256(&bundle)
                        .is_ok_and(|sha256| sha256 == bundle.bundle_sha256)
            }) {
                cloud_bundle_keys.insert(bundle.component.component_key.clone());
            }
        }
    }
    let mut auth_connection_ids = match installation.as_ref() {
        Some(installation) => state
            .store
            .list_plugin_oauth_connections(
                owner_user_id,
                installation.device_id.as_str(),
                catalog.id.as_str(),
            )
            .await
            .map_err(ApiError::internal)?
            .into_iter()
            .filter(|connection| {
                connection.connected && connection.release_id == installation.release_id
            })
            .map(|connection| connection.id)
            .collect(),
        None => Vec::new(),
    };
    if let Some(release) = release.as_ref() {
        auth_connection_ids.extend(
            state
                .store
                .list_plugin_cloud_oauth_connections(
                    owner_user_id,
                    catalog.id.as_str(),
                    release.id.as_str(),
                )
                .await
                .map_err(ApiError::internal)?
                .into_iter()
                .filter(|record| {
                    record.connection.connected
                        && !record.connection.needs_auth
                        && (record.connection.refreshable
                            || record.connection.expires_at.as_deref().is_none_or(|value| {
                                chrono::DateTime::parse_from_rfc3339(value).is_ok_and(|expiry| {
                                    expiry.timestamp() > chrono::Utc::now().timestamp()
                                })
                            }))
                })
                .map(|record| record.connection.id),
        );
    }
    auth_connection_ids.sort();
    auth_connection_ids.dedup();
    let portable_uses_local = portable_uses_local(runtime_provider, binding.agent_key.as_str());
    Ok(Some(resolve_plugin_records(
        catalog,
        release,
        binding,
        installation,
        preference,
        component_snapshots,
        auth_connection_ids,
        device_id,
        &cloud_bundle_keys,
        portable_uses_local,
    )))
}

pub(super) async fn plugin_component_gate(
    state: &AppState,
    ownership: &PluginComponentOwnership,
    owner_user_id: &str,
    device_id: Option<&str>,
    runtime_provider: Option<&str>,
) -> Result<Option<PluginComponentGate>, ApiError> {
    if !ownership.managed_by_plugin {
        return Ok(None);
    }
    let Some((plugin_id, release_id, component_key)) = ownership.complete_identity() else {
        return Ok(Some(PluginComponentGate {
            available: false,
            reason: Some("Plugin component ownership identity is incomplete".to_string()),
        }));
    };
    let binding = AgentBindingRecord {
        id: format!("plugin-component-gate:{plugin_id}:{component_key}"),
        agent_key: "plugin-component-gate".to_string(),
        binding_scope: BINDING_SCOPE_SYSTEM_REQUIRED.to_string(),
        owner_user_id: Some(owner_user_id.to_string()),
        resource_kind: RESOURCE_KIND_PLUGIN_COMPONENT.to_string(),
        resource_id: plugin_id.to_string(),
        enabled: true,
        required: true,
        priority: 0,
        conditions: BindingConditions::default(),
        component_allowlist: vec![component_key.to_string()],
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
    };
    let Some(resolved) =
        resolve_plugin_binding(state, binding, owner_user_id, device_id, runtime_provider).await?
    else {
        return Ok(Some(PluginComponentGate {
            available: false,
            reason: Some("Plugin component catalog entry is missing".to_string()),
        }));
    };
    if resolved.release.as_ref().map(|release| release.id.as_str()) != Some(release_id) {
        return Ok(Some(PluginComponentGate {
            available: false,
            reason: Some(
                "Plugin component Release does not match the active installation".to_string(),
            ),
        }));
    }
    let component = resolved
        .components
        .iter()
        .find(|component| component.component.component_key == component_key);
    Ok(Some(match component {
        Some(component) if resolved.available && component.available => PluginComponentGate {
            available: true,
            reason: None,
        },
        Some(component) => PluginComponentGate {
            available: false,
            reason: component.reason.clone().or(resolved.reason),
        },
        None => PluginComponentGate {
            available: false,
            reason: Some("Plugin component is not selected by the effective policy".to_string()),
        },
    }))
}

fn resolve_plugin_records(
    catalog: PluginCatalogRecord,
    release: Option<PluginReleaseRecord>,
    binding: AgentBindingRecord,
    installation: Option<PluginInstallationRecord>,
    preference: Option<UserPluginPreferenceRecord>,
    component_snapshots: Vec<PluginComponentSnapshot>,
    auth_connection_ids: Vec<String>,
    device_id: Option<&str>,
    cloud_bundle_keys: &HashSet<String>,
    portable_uses_local: bool,
) -> ResolvedPlugin {
    let component_result = resolve_components(
        release.as_ref(),
        installation.as_ref(),
        preference.as_ref(),
        &binding,
        cloud_bundle_keys,
        portable_uses_local,
    );
    let components = component_result.as_ref().cloned().unwrap_or_default();
    let unavailable = |status, reason: String| ResolvedPlugin {
        catalog: catalog.clone(),
        release: release.clone(),
        binding: binding.clone(),
        installation: installation.clone(),
        preference: preference.clone(),
        components: components.clone(),
        component_snapshots: component_snapshots.clone(),
        auth_connection_ids: auth_connection_ids.clone(),
        available: false,
        status,
        reason: Some(reason),
    };

    if !catalog.enabled {
        return unavailable(
            PluginAvailabilityStatus::Unavailable,
            "Plugin catalog entry is disabled".to_string(),
        );
    }
    let Some(preference_ref) = preference.as_ref() else {
        return unavailable(
            PluginAvailabilityStatus::Unavailable,
            "Plugin user preference is missing".to_string(),
        );
    };
    if !preference_ref.enabled {
        return unavailable(
            PluginAvailabilityStatus::Unavailable,
            "Plugin is disabled by user preference".to_string(),
        );
    }
    let Some(release_ref) = release.as_ref() else {
        return unavailable(
            PluginAvailabilityStatus::Unavailable,
            "Plugin has no active immutable Release".to_string(),
        );
    };
    if release_ref.plugin_id != catalog.id {
        return unavailable(
            PluginAvailabilityStatus::Unavailable,
            "Plugin Release identity does not match Catalog".to_string(),
        );
    }
    if release_ref.revoked_at.is_some() {
        return unavailable(
            PluginAvailabilityStatus::Revoked,
            "Plugin Release is revoked".to_string(),
        );
    }
    if preference_ref.release_channel != release_ref.release_channel {
        return unavailable(
            PluginAvailabilityStatus::Unavailable,
            "active Plugin Release does not match the preferred release channel".to_string(),
        );
    }
    let components = match component_result {
        Ok(components) => components,
        Err(reason) => {
            return unavailable(PluginAvailabilityStatus::Unavailable, reason);
        }
    };
    let requires_local = components.iter().any(|component| {
        component.component.execution_host == PluginExecutionHost::Local
            || (component.component.execution_host == PluginExecutionHost::Portable
                && portable_uses_local)
    });
    if requires_local {
        let Some(device_id) = device_id else {
            return unavailable(
                PluginAvailabilityStatus::Unavailable,
                "device_id is required for selected local Plugin components".to_string(),
            );
        };
        let Some(installation_ref) = installation.as_ref() else {
            return unavailable(
                PluginAvailabilityStatus::Unavailable,
                format!("Plugin is not installed on device {device_id}"),
            );
        };
        if installation_ref.plugin_id != catalog.id
            || installation_ref.release_id != release_ref.id
            || installation_ref.version != release_ref.version
            || installation_ref.artifact_sha256 != release_ref.artifact_sha256
        {
            return unavailable(
                PluginAvailabilityStatus::Unavailable,
                "Plugin installation identity, version, or artifact hash does not match Release"
                    .to_string(),
            );
        }
        if !release_ref.supported_platforms.is_empty()
            && !release_ref
                .supported_platforms
                .iter()
                .any(|platform| platform == &installation_ref.platform)
        {
            return unavailable(
                PluginAvailabilityStatus::UnsupportedPlatform,
                "Plugin installation platform is not supported by the active Release".to_string(),
            );
        }
        if !installation_ref.active
            || installation_ref.install_status != PluginInstallStatus::Installed
        {
            return unavailable(
                PluginAvailabilityStatus::Unavailable,
                "Plugin installation is not active and installed".to_string(),
            );
        }
        if installation_ref.dependency_status != PluginRequirementStatus::Satisfied {
            return unavailable(
                PluginAvailabilityStatus::NeedsDependency,
                "Plugin dependencies are not satisfied".to_string(),
            );
        }
        if installation_ref.permission_status != PluginRequirementStatus::Satisfied {
            return unavailable(
                PluginAvailabilityStatus::NeedsPermission,
                "Plugin permissions are not satisfied".to_string(),
            );
        }
        if installation_ref.auth_status != PluginRequirementStatus::Satisfied {
            return unavailable(
                PluginAvailabilityStatus::NeedsAuth,
                "Plugin authentication is not satisfied".to_string(),
            );
        }
    }
    let immutable_components = component_snapshots
        .iter()
        .map(|snapshot| (snapshot.component.component_key.as_str(), snapshot))
        .collect::<std::collections::HashMap<_, _>>();
    if let Some(component) = components.iter().find(|component| {
        immutable_components
            .get(component.component.component_key.as_str())
            .is_none_or(|snapshot| {
                snapshot.plugin_id != catalog.id
                    || snapshot.release_id
                        != release
                            .as_ref()
                            .map(|release| release.id.as_str())
                            .unwrap_or_default()
                    || snapshot.component != component.component
                    || snapshot.content_sha256.trim().is_empty()
            })
    }) {
        return unavailable(
            PluginAvailabilityStatus::Unavailable,
            format!(
                "immutable Plugin component snapshot is missing or mismatched: {}",
                component.component.component_key
            ),
        );
    }
    let available_count = components
        .iter()
        .filter(|component| component.available)
        .count();
    if available_count != components.len() {
        let status = if available_count > 0 {
            PluginAvailabilityStatus::PartiallyAvailable
        } else {
            components
                .first()
                .map(|component| component.status)
                .unwrap_or(PluginAvailabilityStatus::Unavailable)
        };
        return ResolvedPlugin {
            catalog,
            release,
            binding,
            installation,
            preference,
            components,
            component_snapshots,
            auth_connection_ids,
            available: false,
            status,
            reason: Some("one or more selected Plugin components are unavailable".to_string()),
        };
    }
    ResolvedPlugin {
        catalog,
        release,
        binding,
        installation,
        preference,
        components,
        component_snapshots,
        auth_connection_ids,
        available: true,
        status: PluginAvailabilityStatus::Ready,
        reason: None,
    }
}

fn portable_uses_local(runtime_provider: Option<&str>, agent_key: &str) -> bool {
    match runtime_provider
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("local_connector") => true,
        Some(_) => false,
        None => {
            agent_key == "task_runner_local_plan_phase"
                || agent_key == "task_runner_local_run_phase"
        }
    }
}

fn resolve_components(
    release: Option<&PluginReleaseRecord>,
    installation: Option<&PluginInstallationRecord>,
    preference: Option<&UserPluginPreferenceRecord>,
    binding: &AgentBindingRecord,
    cloud_bundle_keys: &HashSet<String>,
    portable_uses_local: bool,
) -> Result<Vec<ResolvedPluginComponent>, String> {
    let Some(release) = release else {
        return Ok(Vec::new());
    };
    let known = release
        .components
        .iter()
        .map(|component| component.component_key.as_str())
        .collect::<HashSet<_>>();
    let binding_allowlist = normalized_component_keys(&binding.component_allowlist);
    if binding.resource_kind == RESOURCE_KIND_PLUGIN_COMPONENT && binding_allowlist.is_empty() {
        return Err("plugin_component binding requires a component_allowlist".to_string());
    }
    if let Some(unknown) = binding_allowlist
        .iter()
        .find(|component_key| !known.contains(component_key.as_str()))
    {
        return Err(format!(
            "Plugin binding references unknown component: {unknown}"
        ));
    }
    let preference_allowlist = preference
        .map(|preference| normalized_component_keys(&preference.enabled_components))
        .unwrap_or_default();
    if let Some(unknown) = preference_allowlist
        .iter()
        .find(|component_key| !known.contains(component_key.as_str()))
    {
        return Err(format!(
            "Plugin preference references a component outside the active Release: {unknown}"
        ));
    }
    let mut selected = if binding_allowlist.is_empty() {
        release
            .components
            .iter()
            .map(|component| component.component_key.clone())
            .collect::<Vec<_>>()
    } else {
        binding_allowlist
    };
    if !preference_allowlist.is_empty() {
        let enabled = preference_allowlist.into_iter().collect::<HashSet<_>>();
        selected.retain(|component_key| enabled.contains(component_key));
    }
    if selected.is_empty() {
        return Err("effective Plugin component selection is empty".to_string());
    }
    let statuses = installation
        .map(|installation| {
            installation
                .component_statuses
                .iter()
                .map(|status| (status.component_key.as_str(), status))
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();
    Ok(release
        .components
        .iter()
        .filter(|component| selected.contains(&component.component_key))
        .map(|component| {
            let cloud_execution = component.execution_host == PluginExecutionHost::Cloud
                || (component.execution_host == PluginExecutionHost::Portable
                    && !portable_uses_local);
            if cloud_execution {
                return if cloud_bundle_keys.contains(component.component_key.as_str()) {
                    ResolvedPluginComponent {
                        component: component.clone(),
                        available: true,
                        status: PluginAvailabilityStatus::Ready,
                        reason: None,
                    }
                } else {
                    ResolvedPluginComponent {
                        component: component.clone(),
                        available: false,
                        status: PluginAvailabilityStatus::Unavailable,
                        reason: Some("immutable cloud runtime Bundle is missing".to_string()),
                    }
                };
            }
            match statuses.get(component.component_key.as_str()) {
                Some(status) if status.kind == component.kind => ResolvedPluginComponent {
                    component: component.clone(),
                    available: status.availability_status == PluginAvailabilityStatus::Ready,
                    status: status.availability_status,
                    reason: status.last_error.clone(),
                },
                Some(_) => ResolvedPluginComponent {
                    component: component.clone(),
                    available: false,
                    status: PluginAvailabilityStatus::Unavailable,
                    reason: Some(
                        "Plugin component status kind does not match the active Release"
                            .to_string(),
                    ),
                },
                None => ResolvedPluginComponent {
                    component: component.clone(),
                    available: false,
                    status: PluginAvailabilityStatus::Unavailable,
                    reason: Some(
                        "Plugin component status is missing from installation".to_string(),
                    ),
                },
            }
        })
        .collect())
}

fn normalized_component_keys(values: &[String]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

#[cfg(test)]
mod tests;
