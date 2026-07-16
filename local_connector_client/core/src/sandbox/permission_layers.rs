// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod loading;
mod resolution;
mod revision;
mod selection;

use std::collections::{BTreeMap, BTreeSet};

use chatos_sandbox_contract::{
    CodexPermissionProfileDocument, PermissionProfileConfiguration, PermissionProfileProvenance,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct RuntimePermissionProfileLayers {
    system: Option<CodexPermissionProfileDocument>,
    user: Option<CodexPermissionProfileDocument>,
    managed: Option<CodexPermissionProfileDocument>,
    load_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct EffectivePermissionProfileConfiguration {
    pub(crate) configuration: PermissionProfileConfiguration,
    pub(crate) default_profile_name: String,
    pub(crate) default_provenance: PermissionProfileProvenance,
    pub(crate) profile_provenance: BTreeMap<String, PermissionProfileProvenance>,
    pub(crate) managed_profile_names: BTreeSet<String>,
    managed_allowed_profile_names: BTreeSet<String>,
}

impl EffectivePermissionProfileConfiguration {
    pub(crate) fn provenance_for(&self, profile_name: &str) -> PermissionProfileProvenance {
        self.profile_provenance
            .get(profile_name)
            .copied()
            .unwrap_or(PermissionProfileProvenance::BuiltIn)
    }

    pub(crate) fn api_locked_profile_names(&self) -> BTreeSet<String> {
        let mut locked = self.managed_profile_names.clone();
        locked.extend(
            self.managed_allowed_profile_names
                .iter()
                .filter(|name| !name.starts_with(':'))
                .cloned(),
        );
        let mut pending = locked.iter().cloned().collect::<Vec<_>>();
        while let Some(profile_name) = pending.pop() {
            let Some(parent) = self
                .configuration
                .profiles
                .get(&profile_name)
                .and_then(|profile| profile.extends.as_ref())
                .filter(|parent| !parent.starts_with(':'))
            else {
                continue;
            };
            if locked.insert(parent.clone()) {
                pending.push(parent.clone());
            }
        }
        locked
    }
}

#[cfg(test)]
impl RuntimePermissionProfileLayers {
    pub(crate) fn for_tests(
        system: Option<CodexPermissionProfileDocument>,
        user: Option<CodexPermissionProfileDocument>,
        managed: Option<CodexPermissionProfileDocument>,
    ) -> Self {
        Self {
            system,
            user,
            managed,
            load_error: None,
        }
    }
}

#[cfg(test)]
mod tests;
