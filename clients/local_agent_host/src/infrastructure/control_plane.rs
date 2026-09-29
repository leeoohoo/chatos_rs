// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    LocalCapabilityResolver, LocalModelRuntimeResolver, ResolvedLocalCapabilities,
    TransientLocalModelRuntime,
};
use async_trait::async_trait;
use chatos_local_agent_ports::{LocalCapabilityPolicySnapshot, LocalCapabilitySnapshotStore};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

type RevisionKey = (String, String);

/// Control-plane registry populated by the native client's authenticated
/// configuration service. Credential-bearing model runtimes remain strictly
/// process-local, while an optional storage port retains non-secret capability
/// revisions across Host restarts.
#[derive(Default)]
pub struct LocalControlPlaneSnapshot {
    models: RwLock<HashMap<RevisionKey, TransientLocalModelRuntime>>,
    capabilities: RwLock<HashMap<RevisionKey, ResolvedLocalCapabilities>>,
    capability_store: Option<Arc<dyn LocalCapabilitySnapshotStore>>,
}

impl LocalControlPlaneSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capability_store(store: Arc<dyn LocalCapabilitySnapshotStore>) -> Self {
        Self {
            capability_store: Some(store),
            ..Self::default()
        }
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

    pub async fn publish_capabilities(
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
        let snapshot = encode_capabilities(&key, &capabilities);
        snapshot.validate()?;
        if let Some(store) = &self.capability_store {
            store
                .put_capability_snapshot(&snapshot, now_unix_ms()?)
                .await
                .map_err(|error| error.to_string())?;
        }
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

    /// Removes only the process-local cache entry. A configured durable store
    /// remains authoritative so active Runs can still resolve the revision.
    pub fn evict_capabilities(
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
        if let Some(capabilities) = self
            .capabilities
            .read()
            .map_err(|_| "capability snapshot lock is poisoned".to_string())?
            .get(&key)
            .cloned()
        {
            return Ok(capabilities);
        }
        let Some(store) = &self.capability_store else {
            return Err(format!(
                "capability revision is not loaded: {profile_key}@{capability_policy_revision}"
            ));
        };
        let stored = store
            .get_capability_snapshot(profile_key, capability_policy_revision)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "capability revision is not loaded: {profile_key}@{capability_policy_revision}"
                )
            })?;
        let capabilities = decode_capabilities(stored);
        self.capabilities
            .write()
            .map_err(|_| "capability snapshot lock is poisoned".to_string())?
            .insert(key, capabilities.clone());
        Ok(capabilities)
    }
}

fn encode_capabilities(
    key: &RevisionKey,
    capabilities: &ResolvedLocalCapabilities,
) -> LocalCapabilityPolicySnapshot {
    LocalCapabilityPolicySnapshot {
        profile_key: key.0.clone(),
        capability_policy_revision: key.1.clone(),
        instructions: capabilities.instructions.clone(),
        prefixed_input_items: capabilities.prefixed_input_items.clone(),
        tools: capabilities.tools.clone(),
    }
}

fn decode_capabilities(snapshot: LocalCapabilityPolicySnapshot) -> ResolvedLocalCapabilities {
    ResolvedLocalCapabilities {
        instructions: snapshot.instructions,
        prefixed_input_items: snapshot.prefixed_input_items,
        tools: snapshot.tools,
    }
}

fn now_unix_ms() -> Result<i64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?;
    i64::try_from(elapsed.as_millis())
        .map_err(|_| "system time does not fit in an i64 millisecond timestamp".to_string())
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
            .await
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
            .evict_capabilities(MAIN_CHAT_PROFILE_KEY, "policy-1")
            .expect("remove"));
    }

    #[tokio::test]
    async fn capabilities_reload_from_sqlite_after_process_cache_is_recreated() {
        let storage = Arc::new(
            chatos_client_storage::SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let first = LocalControlPlaneSnapshot::with_capability_store(storage.clone());
        first
            .publish_capabilities(
                MAIN_CHAT_PROFILE_KEY,
                "policy-durable",
                ResolvedLocalCapabilities {
                    instructions: Some("durable policy".to_string()),
                    tools: vec![serde_json::json!({"name": "read_file"})],
                    ..ResolvedLocalCapabilities::default()
                },
            )
            .await
            .expect("publish");

        let restarted = LocalControlPlaneSnapshot::with_capability_store(storage);
        let resolved = restarted
            .resolve_capabilities(MAIN_CHAT_PROFILE_KEY, "policy-durable")
            .await
            .expect("resolve persisted revision");
        assert_eq!(resolved.instructions.as_deref(), Some("durable policy"));
        assert_eq!(
            resolved.tools,
            vec![serde_json::json!({"name": "read_file"})]
        );
    }

    #[tokio::test]
    async fn oversized_capabilities_are_rejected_before_entering_memory() {
        let snapshot = LocalControlPlaneSnapshot::new();
        let error = snapshot
            .publish_capabilities(
                MAIN_CHAT_PROFILE_KEY,
                "policy-too-large",
                ResolvedLocalCapabilities {
                    instructions: Some(
                        "x".repeat(chatos_local_agent_ports::MAX_CAPABILITY_INSTRUCTIONS_BYTES + 1),
                    ),
                    ..ResolvedLocalCapabilities::default()
                },
            )
            .await
            .expect_err("oversized snapshot must fail");
        assert!(error.contains("instructions"));
    }
}
