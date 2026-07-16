// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use chatos_sandbox_contract::{
    CodexPermissionProfileDocument, CustomPermissionProfile, PermissionProfileConfiguration,
    PermissionProfileId, PermissionProfileProvenance,
};

use super::loading::merge_optional_documents;
use super::selection::{builtin_profile_id, select_default_profile};
use super::{EffectivePermissionProfileConfiguration, RuntimePermissionProfileLayers};

impl RuntimePermissionProfileLayers {
    #[cfg(test)]
    pub(crate) fn effective_configuration(
        &self,
        persisted_profiles: &BTreeMap<String, CustomPermissionProfile>,
        persisted_allowed_profiles: Option<&BTreeMap<String, bool>>,
        persisted_default_profile_name: Option<&str>,
        legacy_default_profile_id: PermissionProfileId,
    ) -> Result<EffectivePermissionProfileConfiguration> {
        self.effective_configuration_with_project(
            persisted_profiles,
            persisted_allowed_profiles,
            persisted_default_profile_name,
            legacy_default_profile_id,
            None,
        )
    }

    pub(crate) fn effective_configuration_with_project(
        &self,
        persisted_profiles: &BTreeMap<String, CustomPermissionProfile>,
        persisted_allowed_profiles: Option<&BTreeMap<String, bool>>,
        persisted_default_profile_name: Option<&str>,
        legacy_default_profile_id: PermissionProfileId,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> Result<EffectivePermissionProfileConfiguration> {
        if let Some(error) = self.load_error.as_deref() {
            return Err(anyhow!(
                "managed requirements are unresolved and sandbox permissions are blocked: {error}"
            ));
        }
        let configured_default_provenance = if project
            .and_then(|document| document.default_permissions.as_ref())
            .is_some()
        {
            PermissionProfileProvenance::Project
        } else if persisted_default_profile_name.is_some()
            || self
                .user
                .as_ref()
                .and_then(|document| document.default_permissions.as_ref())
                .is_some()
            || self
                .system
                .as_ref()
                .and_then(|document| document.default_permissions.as_ref())
                .is_some()
        {
            PermissionProfileProvenance::User
        } else {
            PermissionProfileProvenance::BuiltIn
        };
        let project_profile_names = project
            .into_iter()
            .flat_map(|document| document.configuration.profiles.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let persisted_document = (!persisted_profiles.is_empty()
            || persisted_allowed_profiles.is_some()
            || persisted_default_profile_name.is_some())
        .then(|| CodexPermissionProfileDocument {
            default_permissions: persisted_default_profile_name.map(ToOwned::to_owned),
            configuration: PermissionProfileConfiguration {
                profiles: persisted_profiles.clone(),
                allowed_permission_profiles: persisted_allowed_profiles.cloned(),
            },
        });
        let mut ordinary = merge_optional_documents(self.system.clone(), self.user.clone())?;
        ordinary = merge_optional_documents(ordinary, persisted_document)?;
        let pre_project_allowed_profiles = ordinary
            .as_ref()
            .and_then(|document| document.configuration.allowed_permission_profiles.clone());
        let project_allowed_profiles =
            project.and_then(|document| document.configuration.allowed_permission_profiles.clone());
        ordinary = merge_optional_documents(ordinary, project.cloned())?;
        let mut ordinary = ordinary.unwrap_or_default();
        ordinary.configuration.allowed_permission_profiles =
            intersect_complete_allowlists(pre_project_allowed_profiles, project_allowed_profiles);

        let mut profile_provenance = ordinary
            .configuration
            .profiles
            .keys()
            .map(|name| (name.clone(), PermissionProfileProvenance::User))
            .collect::<BTreeMap<_, _>>();
        for profile_name in project_profile_names {
            profile_provenance.insert(profile_name, PermissionProfileProvenance::Project);
        }
        let mut managed_profile_names = BTreeSet::new();
        let managed_allowed_profiles = self
            .managed
            .as_ref()
            .and_then(|document| document.configuration.allowed_permission_profiles.as_ref());
        let managed_allowed_profile_names = managed_allowed_profiles
            .into_iter()
            .flat_map(|allowed| allowed.iter())
            .filter(|(_, enabled)| **enabled)
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>();

        if let Some(managed) = self.managed.as_ref() {
            for (profile_name, profile) in &managed.configuration.profiles {
                if ordinary.configuration.profiles.contains_key(profile_name) {
                    return Err(anyhow!(
                        "managed permission profile {profile_name:?} conflicts with a config-defined profile of the same name"
                    ));
                }
                ordinary
                    .configuration
                    .profiles
                    .insert(profile_name.clone(), profile.clone());
                profile_provenance
                    .insert(profile_name.clone(), PermissionProfileProvenance::Managed);
                managed_profile_names.insert(profile_name.clone());
            }
        }

        validate_managed_requirements(self.managed.as_ref(), &ordinary.configuration.profiles)?;
        validate_ordinary_allowlist(
            ordinary.configuration.allowed_permission_profiles.as_ref(),
            &ordinary.configuration.profiles,
        )?;

        ordinary.configuration.allowed_permission_profiles = compose_allowed_profiles(
            ordinary.configuration.allowed_permission_profiles.as_ref(),
            managed_allowed_profiles,
            &ordinary.configuration.profiles,
        );
        ordinary
            .configuration
            .validate()
            .map_err(anyhow::Error::msg)?;

        let configured_default = ordinary
            .default_permissions
            .clone()
            .unwrap_or_else(|| legacy_default_profile_id.codex_name().to_string());
        let managed_fallback = managed_default_profile(self.managed.as_ref())?;
        let (default_profile_name, default_provenance) = select_default_profile(
            &ordinary.configuration,
            configured_default.as_str(),
            managed_fallback,
            &profile_provenance,
            configured_default_provenance,
        )?;

        Ok(EffectivePermissionProfileConfiguration {
            configuration: ordinary.configuration,
            default_profile_name,
            default_provenance,
            profile_provenance,
            managed_profile_names,
            managed_allowed_profile_names,
        })
    }
}

fn validate_managed_requirements(
    managed: Option<&CodexPermissionProfileDocument>,
    available_profiles: &BTreeMap<String, CustomPermissionProfile>,
) -> Result<()> {
    let Some(managed) = managed else {
        return Ok(());
    };
    let Some(allowed) = managed.configuration.allowed_permission_profiles.as_ref() else {
        if managed.default_permissions.is_some() {
            return Err(anyhow!(
                "managed default_permissions requires allowed_permission_profiles"
            ));
        }
        return Ok(());
    };
    if allowed.is_empty() || !allowed.values().any(|enabled| *enabled) {
        return Err(anyhow!(
            "managed allowed_permission_profiles must enable at least one permission profile"
        ));
    }
    for profile_name in allowed.keys() {
        if builtin_profile_id(profile_name).is_none()
            && !available_profiles.contains_key(profile_name)
        {
            return Err(anyhow!(
                "managed allowed_permission_profiles refers to undefined profile {profile_name:?}"
            ));
        }
    }
    let Some(default_profile) = managed_default_profile(Some(managed))? else {
        return Ok(());
    };
    if !allowed.get(default_profile).copied().unwrap_or(false) {
        return Err(anyhow!(
            "managed default_permissions {default_profile:?} must be allowed by allowed_permission_profiles"
        ));
    }
    Ok(())
}

fn validate_ordinary_allowlist(
    allowed: Option<&BTreeMap<String, bool>>,
    available_profiles: &BTreeMap<String, CustomPermissionProfile>,
) -> Result<()> {
    let Some(allowed) = allowed else {
        return Ok(());
    };
    for profile_name in allowed.keys() {
        if builtin_profile_id(profile_name).is_none()
            && !available_profiles.contains_key(profile_name)
        {
            return Err(anyhow!(
                "allowed_permission_profiles refers to undefined profile {profile_name:?}"
            ));
        }
    }
    Ok(())
}

fn managed_default_profile(
    managed: Option<&CodexPermissionProfileDocument>,
) -> Result<Option<&str>> {
    let Some(managed) = managed else {
        return Ok(None);
    };
    let Some(allowed) = managed.configuration.allowed_permission_profiles.as_ref() else {
        return Ok(None);
    };
    if let Some(default_profile) = managed.default_permissions.as_deref() {
        return Ok(Some(default_profile));
    }
    let allows_workspace = allowed
        .get(PermissionProfileId::WorkspaceWrite.codex_name())
        .copied()
        .unwrap_or(false);
    let allows_read_only = allowed
        .get(PermissionProfileId::ReadOnly.codex_name())
        .copied()
        .unwrap_or(false);
    if allows_workspace && allows_read_only {
        Ok(Some(PermissionProfileId::WorkspaceWrite.codex_name()))
    } else {
        Err(anyhow!(
            "managed default_permissions must be set unless allowed_permission_profiles allows both :workspace and :read-only"
        ))
    }
}

fn compose_allowed_profiles(
    ordinary: Option<&BTreeMap<String, bool>>,
    managed: Option<&BTreeMap<String, bool>>,
    profiles: &BTreeMap<String, CustomPermissionProfile>,
) -> Option<BTreeMap<String, bool>> {
    if ordinary.is_none() && managed.is_none() {
        return None;
    }
    let mut names = PermissionProfileId::ALL
        .into_iter()
        .map(|profile| profile.codex_name().to_string())
        .collect::<BTreeSet<_>>();
    names.extend(profiles.keys().cloned());
    Some(
        names
            .into_iter()
            .map(|name| {
                let ordinary_allowed =
                    ordinary.is_none_or(|allowed| allowed.get(&name).copied().unwrap_or(false));
                let managed_allowed =
                    managed.is_none_or(|allowed| allowed.get(&name).copied().unwrap_or(false));
                (name, ordinary_allowed && managed_allowed)
            })
            .collect(),
    )
}

fn intersect_complete_allowlists(
    lower: Option<BTreeMap<String, bool>>,
    higher: Option<BTreeMap<String, bool>>,
) -> Option<BTreeMap<String, bool>> {
    match (lower, higher) {
        (Some(lower), Some(higher)) => {
            let names = lower
                .keys()
                .chain(higher.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            Some(
                names
                    .into_iter()
                    .map(|name| {
                        let lower_allowed = lower.get(&name).copied().unwrap_or(false);
                        let higher_allowed = higher.get(&name).copied().unwrap_or(false);
                        (name, lower_allowed && higher_allowed)
                    })
                    .collect(),
            )
        }
        (Some(allowed), None) | (None, Some(allowed)) => Some(allowed),
        (None, None) => None,
    }
}
