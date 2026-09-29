// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    LocalCapabilityResolver, LocalModelRuntimeResolver, ResolvedLocalCapabilities,
    TransientLocalModelRuntime,
};
use async_trait::async_trait;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

type RevisionKey = (String, String);

/// Process-memory control-plane snapshot populated by the native client's
/// authenticated configuration service. It is intentionally not serializable:
/// model credentials must never enter the durable Run database.
#[derive(Default)]
pub struct LocalControlPlaneSnapshot {
    models: RwLock<HashMap<RevisionKey, TransientLocalModelRuntime>>,
    capabilities: RwLock<HashMap<RevisionKey, ResolvedLocalCapabilities>>,
}

impl LocalControlPlaneSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish_model_runtime(
        &self,
        model_config_ref: impl Into<String>,
        model_config_revision: impl Into<String>,
        runtime: TransientLocalModelRuntime,
    ) -> Result<(), String> {
        let key = revision_key(
            "model_config_ref",
            model_config_ref.into(),
            "model_config_revision",
            model_config_revision.into(),
        )?;
        self.models
            .write()
            .map_err(|_| "model control-plane snapshot lock is poisoned".to_string())?
            .insert(key, runtime);
        Ok(())
    }

    pub fn publish_capabilities(
        &self,
        profile_key: impl Into<String>,
        capability_policy_revision: impl Into<String>,
        capabilities: ResolvedLocalCapabilities,
    ) -> Result<(), String> {
        let key = revision_key(
            "profile_key",
            profile_key.into(),
            "capability_policy_revision",
            capability_policy_revision.into(),
        )?;
        self.capabilities
            .write()
            .map_err(|_| "capability snapshot lock is poisoned".to_string())?
            .insert(key, capabilities);
        Ok(())
    }

    pub fn remove_model_runtime(
        &self,
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<bool, String> {
        let key = revision_key(
            "model_config_ref",
            model_config_ref.to_string(),
            "model_config_revision",
            model_config_revision.to_string(),
        )?;
        Ok(self
            .models
            .write()
            .map_err(|_| "model control-plane snapshot lock is poisoned".to_string())?
            .remove(&key)
            .is_some())
    }

    pub fn remove_capabilities(
        &self,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<bool, String> {
        let key = revision_key(
            "profile_key",
            profile_key.to_string(),
            "capability_policy_revision",
            capability_policy_revision.to_string(),
        )?;
        Ok(self
            .capabilities
            .write()
            .map_err(|_| "capability snapshot lock is poisoned".to_string())?
            .remove(&key)
            .is_some())
    }
}

#[async_trait]
impl LocalModelRuntimeResolver for LocalControlPlaneSnapshot {
    async fn resolve_model_runtime(
        &self,
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<TransientLocalModelRuntime, String> {
        let key = revision_key(
            "model_config_ref",
            model_config_ref.to_string(),
            "model_config_revision",
            model_config_revision.to_string(),
        )?;
        let models = self
            .models
            .read()
            .map_err(|_| "model control-plane snapshot lock is poisoned".to_string())?;
        let runtime = models.get(&key).ok_or_else(|| {
            format!(
                "model runtime revision is not loaded: {model_config_ref}@{model_config_revision}"
            )
        })?;
        Ok(TransientLocalModelRuntime {
            runner: Arc::clone(&runtime.runner),
            model_config: runtime.model_config.clone(),
        })
    }
}

#[async_trait]
impl LocalCapabilityResolver for LocalControlPlaneSnapshot {
    async fn resolve_capabilities(
        &self,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<ResolvedLocalCapabilities, String> {
        let key = revision_key(
            "profile_key",
            profile_key.to_string(),
            "capability_policy_revision",
            capability_policy_revision.to_string(),
        )?;
        self.capabilities
            .read()
            .map_err(|_| "capability snapshot lock is poisoned".to_string())?
            .get(&key)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "capability revision is not loaded: {profile_key}@{capability_policy_revision}"
                )
            })
    }
}

fn revision_key(
    first_name: &str,
    first: String,
    second_name: &str,
    second: String,
) -> Result<RevisionKey, String> {
    for (name, value) in [(first_name, first.as_str()), (second_name, second.as_str())] {
        if value.trim().is_empty() || value.len() > 256 {
            return Err(format!("{name} must be 1..=256 characters"));
        }
    }
    Ok((first, second))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MAIN_CHAT_PROFILE_KEY, TASK_RUNNER_PROFILE_KEY};

    #[tokio::test]
    async fn capabilities_are_resolved_by_exact_profile_and_revision() {
        let snapshot = LocalControlPlaneSnapshot::new();
        snapshot
            .publish_capabilities(
                MAIN_CHAT_PROFILE_KEY,
                "policy-1",
                ResolvedLocalCapabilities {
                    instructions: Some("main chat".to_string()),
                    ..ResolvedLocalCapabilities::default()
                },
            )
            .expect("publish");
        let resolved = snapshot
            .resolve_capabilities(MAIN_CHAT_PROFILE_KEY, "policy-1")
            .await
            .expect("resolve");
        assert_eq!(resolved.instructions.as_deref(), Some("main chat"));
        assert!(snapshot
            .resolve_capabilities(TASK_RUNNER_PROFILE_KEY, "policy-1")
            .await
            .is_err());
        assert!(snapshot
            .resolve_capabilities(MAIN_CHAT_PROFILE_KEY, "policy-2")
            .await
            .is_err());
        assert!(snapshot
            .remove_capabilities(MAIN_CHAT_PROFILE_KEY, "policy-1")
            .expect("remove"));
    }
}
