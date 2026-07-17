// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use chatos_sandbox_contract::{
    PermissionProfileConfiguration, PermissionProfileId, PermissionProfileProvenance,
};

pub(super) fn select_default_profile(
    configuration: &PermissionProfileConfiguration,
    configured_default: &str,
    managed_fallback: Option<&str>,
    profile_provenance: &BTreeMap<String, PermissionProfileProvenance>,
    configured_default_provenance: PermissionProfileProvenance,
) -> Result<(String, PermissionProfileProvenance)> {
    if configuration.profile_allowed(configured_default)
        && configuration
            .resolve(
                configured_default,
                Vec::new(),
                None,
                configured_selection_provenance(
                    profile_provenance,
                    configured_default,
                    configured_default_provenance,
                ),
            )
            .is_ok()
    {
        return Ok((
            configured_default.to_string(),
            configured_selection_provenance(
                profile_provenance,
                configured_default,
                configured_default_provenance,
            ),
        ));
    }
    if let Some(managed_fallback) = managed_fallback {
        if configuration.profile_allowed(managed_fallback)
            && configuration
                .resolve(
                    managed_fallback,
                    Vec::new(),
                    None,
                    PermissionProfileProvenance::Managed,
                )
                .is_ok()
        {
            return Ok((
                managed_fallback.to_string(),
                managed_selection_provenance(profile_provenance, managed_fallback),
            ));
        }
    }

    let mut candidates = configuration
        .catalog()
        .into_iter()
        .filter(|entry| entry.allowed)
        .filter_map(|entry| {
            configuration
                .resolve(
                    entry.id.as_str(),
                    Vec::new(),
                    None,
                    provenance_for_profile(profile_provenance, entry.id.as_str()),
                )
                .ok()
                .map(|resolved| (resolved.permission_profile_id.rank(), entry.id))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    let Some((_, profile_name)) = candidates.into_iter().next() else {
        return Err(anyhow!(
            "effective allowed_permission_profiles does not leave a resolvable permission profile"
        ));
    };
    let provenance = if managed_fallback.is_some() {
        managed_selection_provenance(profile_provenance, profile_name.as_str())
    } else {
        provenance_for_profile(profile_provenance, profile_name.as_str())
    };
    Ok((profile_name, provenance))
}

fn configured_selection_provenance(
    provenance: &BTreeMap<String, PermissionProfileProvenance>,
    profile_name: &str,
    configured_default_provenance: PermissionProfileProvenance,
) -> PermissionProfileProvenance {
    provenance
        .get(profile_name)
        .copied()
        .unwrap_or(configured_default_provenance)
}

fn managed_selection_provenance(
    provenance: &BTreeMap<String, PermissionProfileProvenance>,
    profile_name: &str,
) -> PermissionProfileProvenance {
    provenance
        .get(profile_name)
        .copied()
        .unwrap_or(PermissionProfileProvenance::Managed)
}

fn provenance_for_profile(
    provenance: &BTreeMap<String, PermissionProfileProvenance>,
    profile_name: &str,
) -> PermissionProfileProvenance {
    provenance
        .get(profile_name)
        .copied()
        .unwrap_or(PermissionProfileProvenance::BuiltIn)
}

pub(super) fn builtin_profile_id(name: &str) -> Option<PermissionProfileId> {
    PermissionProfileId::ALL
        .into_iter()
        .find(|profile| profile.codex_name() == name)
}
