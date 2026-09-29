// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    LocalCapabilityResolver, LocalModelRuntimeResolver, ResolvedLocalCapabilities,
    TransientLocalModelRuntime,
};
use async_trait::async_trait;
use chatos_ai_runtime::{ContextualTurnRunner, JsonSchemaOutputFormat, ModelRuntimeConfig};
use chatos_local_agent_ports::{
    IdempotentCommand, LocalCapabilityPolicySnapshot, LocalCapabilitySnapshotStore,
    LocalJsonSchemaOutputFormat, LocalModelConfigSnapshot, LocalModelConfigSnapshotStore,
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

type RevisionKey = (String, String);

#[async_trait]
pub trait LocalModelCredentialResolver: Send + Sync {
    /// Resolves one native credential-store reference for a single model
    /// request. Implementations must not persist or log the returned secret.
    async fn resolve_model_api_key(&self, credential_ref: &str) -> Result<String, String>;
}

/// Control-plane registry populated by the native client's authenticated
/// configuration service. Credential-bearing model runtimes remain strictly
/// process-local, while an optional storage port retains non-secret capability
/// revisions across Host restarts.
#[derive(Default)]
pub struct LocalControlPlaneSnapshot {
    models: RwLock<HashMap<RevisionKey, TransientLocalModelRuntime>>,
    capabilities: RwLock<HashMap<RevisionKey, ResolvedLocalCapabilities>>,
    capability_store: Option<Arc<dyn LocalCapabilitySnapshotStore>>,
    model_store: Option<Arc<dyn LocalModelConfigSnapshotStore>>,
    model_runner: Option<Arc<ContextualTurnRunner>>,
    model_credentials: Option<Arc<dyn LocalModelCredentialResolver>>,
}

impl LocalControlPlaneSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capability_store(mut self, store: Arc<dyn LocalCapabilitySnapshotStore>) -> Self {
        self.capability_store = Some(store);
        self
    }

    pub fn with_model_store(
        mut self,
        store: Arc<dyn LocalModelConfigSnapshotStore>,
        runner: Arc<ContextualTurnRunner>,
        credentials: Arc<dyn LocalModelCredentialResolver>,
    ) -> Self {
        self.model_store = Some(store);
        self.model_runner = Some(runner);
        self.model_credentials = Some(credentials);
        self
    }

    pub async fn publish_model_config(
        &self,
        snapshot: &LocalModelConfigSnapshot,
    ) -> Result<(), String> {
        snapshot.validate()?;
        let store = self.model_store.as_ref().ok_or_else(|| {
            "model config storage is not configured for this control-plane resolver".to_string()
        })?;
        let command = snapshot_command(
            "model",
            &snapshot.model_config_ref,
            &snapshot.model_config_revision,
            snapshot,
        )?;
        store
            .put_model_config_snapshot(&command, snapshot, now_unix_ms()?)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
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
            let command = snapshot_command(
                "capability",
                &snapshot.profile_key,
                &snapshot.capability_policy_revision,
                &snapshot,
            )?;
            store
                .put_capability_snapshot(&command, &snapshot, now_unix_ms()?)
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
        if let Some(runtime) = self
            .models
            .read()
            .map_err(|_| "model control-plane snapshot lock is poisoned".to_string())?
            .get(&key)
        {
            return Ok(TransientLocalModelRuntime {
                runner: Arc::clone(&runtime.runner),
                model_config: runtime.model_config.clone(),
            });
        }
        let (Some(store), Some(runner), Some(credentials)) = (
            &self.model_store,
            &self.model_runner,
            &self.model_credentials,
        ) else {
            return Err(format!(
                "model runtime revision is not loaded: {model_config_ref}@{model_config_revision}"
            ));
        };
        let snapshot = store
            .get_model_config_snapshot(model_config_ref, model_config_revision)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "model config revision is not loaded: {model_config_ref}@{model_config_revision}"
                )
            })?;
        let api_key = credentials
            .resolve_model_api_key(&snapshot.credential_ref)
            .await?;
        Ok(TransientLocalModelRuntime {
            runner: Arc::clone(runner),
            model_config: decode_model_config(snapshot, api_key)?,
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

fn decode_model_config(
    snapshot: LocalModelConfigSnapshot,
    api_key: String,
) -> Result<ModelRuntimeConfig, String> {
    snapshot.validate()?;
    Ok(ModelRuntimeConfig {
        base_url: snapshot.base_url,
        api_key,
        model: snapshot.model,
        provider: snapshot.provider,
        supports_responses: snapshot.supports_responses,
        supports_images: snapshot.supports_images,
        instructions: snapshot.instructions,
        temperature: snapshot.temperature,
        max_output_tokens: snapshot.max_output_tokens,
        thinking_level: snapshot.thinking_level,
        prompt_cache_key: None,
        previous_response_id: None,
        request_cwd: None,
        include_prompt_cache_retention: snapshot.include_prompt_cache_retention,
        request_body_limit_bytes: snapshot
            .request_body_limit_bytes
            .map(usize::try_from)
            .transpose()
            .map_err(|_| "request_body_limit_bytes does not fit this platform".to_string())?,
        max_transient_retries: snapshot.max_transient_retries.map(|value| value as usize),
        output_format: snapshot.output_format.map(decode_output_format),
    })
}

fn decode_output_format(format: LocalJsonSchemaOutputFormat) -> JsonSchemaOutputFormat {
    JsonSchemaOutputFormat {
        name: format.name,
        description: format.description,
        schema: format.schema,
        strict: format.strict,
    }
}

fn now_unix_ms() -> Result<i64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?;
    i64::try_from(elapsed.as_millis())
        .map_err(|_| "system time does not fit in an i64 millisecond timestamp".to_string())
}

fn snapshot_command<T: serde::Serialize>(
    kind: &str,
    reference: &str,
    revision: &str,
    snapshot: &T,
) -> Result<IdempotentCommand, String> {
    Ok(IdempotentCommand {
        command_id: format!("internal-control-plane-{kind}:{reference}:{revision}"),
        request_fingerprint: serde_json::to_string(snapshot).map_err(|error| error.to_string())?,
    })
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
