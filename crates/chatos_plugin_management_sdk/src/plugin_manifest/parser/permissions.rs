// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

use super::{normalize_optional, normalize_strings};
use crate::plugin_manifest::PluginPermissionRequirement;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum PermissionInput {
    Name(String),
    Detailed(PluginPermissionRequirement),
}

pub(super) fn normalize_permissions(
    inputs: Vec<PermissionInput>,
) -> Vec<PluginPermissionRequirement> {
    inputs
        .into_iter()
        .map(|input| match input {
            PermissionInput::Name(permission) => PluginPermissionRequirement {
                permission: permission.trim().to_string(),
                required: true,
                reason: None,
                components: Vec::new(),
            },
            PermissionInput::Detailed(mut requirement) => {
                requirement.permission = requirement.permission.trim().to_string();
                requirement.reason = normalize_optional(requirement.reason);
                requirement.components = normalize_strings(requirement.components);
                requirement
            }
        })
        .collect()
}
