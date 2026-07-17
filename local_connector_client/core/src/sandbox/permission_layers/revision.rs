// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_sandbox_contract::CodexPermissionProfileDocument;
use sha2::{Digest, Sha256};

use super::RuntimePermissionProfileLayers;

impl RuntimePermissionProfileLayers {
    #[cfg(test)]
    pub(crate) fn effective_policy_revision(
        &self,
        persisted_revision: Option<&str>,
    ) -> Option<String> {
        self.effective_policy_revision_with_project(persisted_revision, None)
    }

    pub(crate) fn effective_policy_revision_with_project(
        &self,
        persisted_revision: Option<&str>,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> Option<String> {
        let runtime_revision = runtime_layers_revision(self, project)?;
        Some(match persisted_revision {
            Some(persisted) => format!("{persisted}+{runtime_revision}"),
            None => runtime_revision,
        })
    }
}

fn runtime_layers_revision(
    layers: &RuntimePermissionProfileLayers,
    project: Option<&CodexPermissionProfileDocument>,
) -> Option<String> {
    if layers.system.is_none()
        && layers.user.is_none()
        && layers.managed.is_none()
        && project.is_none()
    {
        return None;
    }
    let mut hasher = Sha256::new();
    for (label, document) in [
        ("system", layers.system.as_ref()),
        ("user", layers.user.as_ref()),
        ("managed", layers.managed.as_ref()),
        ("project", project),
    ] {
        hasher.update(label.as_bytes());
        hasher.update([0]);
        if let Some(document) = document {
            hasher.update(serde_json::to_vec(document).unwrap_or_default());
        }
        hasher.update([0xff]);
    }
    Some(format!("runtime-{}", hex::encode(hasher.finalize())))
}
