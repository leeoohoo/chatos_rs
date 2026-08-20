// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use chatos_mcp_management_sdk::{
    ProjectExecutionContext, ResolvedMcpRoute, RuntimeSessionRoutesResponse, RuntimeToolDescriptor,
    RuntimeWorkspaceRouteTarget, WorkspaceProviderKind,
};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, spec::BinarySubtype, Binary, DateTime};
use mongodb::options::{IndexOptions, ReplaceOptions};
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::runtime::{
    PluginCloudToolComponentBinding, PluginLocalProviderBinding, PluginLocalToolComponentBinding,
    PluginMcpRuntimeBinding, PluginToolComponentRuntimeBinding,
};

#[path = "session_store/cache.rs"]
mod cache;
#[cfg(test)]
use self::cache::cache_snapshot_with_limits;
pub use self::cache::RuntimeSessionCacheLimits;
use self::cache::{
    cache_snapshot, cache_snapshot_arc, estimate_snapshot_cache_bytes, saturating_u64_to_usize,
    summarize_snapshot_sizes, RuntimeSessionCache,
};
const SNAPSHOT_SCHEMA_VERSION: i32 = 10;
const SNAPSHOT_NONCE_BYTES: usize = 12;
const MAX_PERSISTED_HEADERS: usize = 64;
const MAX_PERSISTED_HEADER_BYTES: usize = 32 * 1024;
const MAX_PERSISTED_TOOL_POLICY_ITEMS: usize = 512;
const MAX_PERSISTED_TOOL_NAME_BYTES: usize = 256;
const MAX_PERSISTED_SNAPSHOT_BYTES: usize = 12 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalConnectorInlineHttpRuntime {
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalConnectorMcpProviderBinding {
    pub provider_ref: String,
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_http: Option<LocalConnectorInlineHttpRuntime>,
    pub allow_writes: bool,
    #[serde(default)]
    pub allowed_tool_names: HashSet<String>,
    #[serde(default)]
    pub blocked_tool_names: HashSet<String>,
}

impl LocalConnectorMcpProviderBinding {
    pub fn allows_tool(&self, tool_name: &str) -> bool {
        let tool_name = tool_name.trim();
        !tool_name.is_empty()
            && (self.allowed_tool_names.is_empty() || self.allowed_tool_names.contains(tool_name))
            && !self.blocked_tool_names.contains(tool_name)
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeSessionSnapshot {
    pub session_id: String,
    pub caller_service: String,
    pub trace_id: String,
    pub tenant_id: String,
    pub owner_user_id: String,
    pub owner_role: Option<String>,
    pub agent_key: String,
    pub task_profile: Option<String>,
    pub project_id: String,
    pub device_id: Option<String>,
    pub run_id: Option<String>,
    pub execution_group_id: Option<String>,
    pub execution_scope_generation: Option<i64>,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub default_model_config_id: Option<String>,
    pub tool_result_max_chars: Option<usize>,
    pub expected_project_task_ids: Vec<String>,
    pub workspace_route: Option<RuntimeWorkspaceRouteTarget>,
    pub project_context: ProjectExecutionContext,
    pub policy_revision: String,
    pub route_revision: String,
    pub routes: Vec<ResolvedMcpRoute>,
    pub tools: Vec<RuntimeToolDescriptor>,
    pub effective_mcp_ids: Vec<String>,
    pub provider_skills_prompt: Option<String>,
    pub plugin_instruction_items: Vec<serde_json::Value>,
    pub plugin_mcp_bindings: HashMap<String, PluginMcpRuntimeBinding>,
    pub plugin_local_bindings: HashMap<String, PluginLocalProviderBinding>,
    pub plugin_tool_component_bindings: HashMap<String, PluginToolComponentRuntimeBinding>,
    pub plugin_local_tool_component_bindings: HashMap<String, PluginLocalToolComponentBinding>,
    pub plugin_cloud_tool_component_bindings: HashMap<String, PluginCloudToolComponentBinding>,
    pub local_connector_mcp_bindings: HashMap<String, LocalConnectorMcpProviderBinding>,
    pub expires_at: String,
    pub expires_at_unix: i64,
}

impl RuntimeSessionSnapshot {
    pub fn execution_scope_provider(&self) -> WorkspaceProviderKind {
        self.workspace_route
            .as_ref()
            .map(RuntimeWorkspaceRouteTarget::provider_kind)
            .unwrap_or(self.project_context.workspace_provider)
    }

    pub fn routes_response(&self) -> RuntimeSessionRoutesResponse {
        RuntimeSessionRoutesResponse {
            session_id: self.session_id.clone(),
            tenant_id: self.tenant_id.clone(),
            owner_user_id: self.owner_user_id.clone(),
            agent_key: self.agent_key.clone(),
            project_id: self.project_id.clone(),
            device_id: self.device_id.clone(),
            run_id: self.run_id.clone(),
            execution_group_id: self.execution_group_id.clone(),
            task_id: self.task_id.clone(),
            task_profile: self.task_profile.clone(),
            workspace_route: self.workspace_route.clone(),
            policy_revision: self.policy_revision.clone(),
            route_revision: self.route_revision.clone(),
            expires_at: self.expires_at.clone(),
            routes: self.routes.clone(),
            tools: self.tools.clone(),
            effective_mcp_ids: self.effective_mcp_ids.clone(),
            provider_skills_prompt: self.provider_skills_prompt.clone(),
            plugin_instruction_items: self.plugin_instruction_items.clone(),
            mcp_command_queue: String::new(),
            mcp_server_url: String::new(),
            runtime_token: String::new(),
        }
    }
}

#[derive(Clone)]
pub struct RuntimeSessionStore {
    backend: Arc<RuntimeSessionStoreBackend>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeSessionStoreStats {
    pub backend: &'static str,
    pub active_session_count: usize,
    pub cached_session_count: usize,
    pub cached_total_bytes: usize,
    pub cached_avg_snapshot_bytes: usize,
    pub cached_p95_snapshot_bytes: usize,
    pub cache_entry_limit: Option<usize>,
    pub cache_byte_limit: Option<usize>,
    pub cache_hits_total: u64,
    pub cache_misses_total: u64,
    pub cache_capacity_evictions_total: u64,
    pub cache_expired_evictions_total: u64,
    pub cache_oversized_rejections_total: u64,
}

enum RuntimeSessionStoreBackend {
    Memory(RwLock<HashMap<String, Arc<RuntimeSessionSnapshot>>>),
    Mongo(MongoRuntimeSessionStore),
}

struct MongoRuntimeSessionStore {
    collection: Collection<StoredRuntimeSessionDocument>,
    cipher: SnapshotCipher,
    cache_limits: RuntimeSessionCacheLimits,
    cache: RwLock<RuntimeSessionCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRuntimeSessionDocument {
    #[serde(rename = "_id")]
    session_id: String,
    schema_version: i32,
    expires_at: DateTime,
    expires_at_unix: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_scope_hash: Option<String>,
    nonce: Binary,
    encrypted_snapshot: Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedRuntimeSessionSnapshot {
    session_id: String,
    caller_service: String,
    trace_id: String,
    tenant_id: String,
    owner_user_id: String,
    #[serde(default)]
    owner_role: Option<String>,
    agent_key: String,
    #[serde(default)]
    task_profile: Option<String>,
    project_id: String,
    device_id: Option<String>,
    run_id: Option<String>,
    #[serde(default)]
    execution_group_id: Option<String>,
    execution_scope_generation: Option<i64>,
    turn_id: Option<String>,
    task_id: Option<String>,
    source_session_id: Option<String>,
    source_user_message_id: Option<String>,
    contact_agent_id: Option<String>,
    default_model_config_id: Option<String>,
    #[serde(default)]
    tool_result_max_chars: Option<usize>,
    expected_project_task_ids: Vec<String>,
    workspace_route: Option<RuntimeWorkspaceRouteTarget>,
    project_context: ProjectExecutionContext,
    policy_revision: String,
    route_revision: String,
    routes: Vec<ResolvedMcpRoute>,
    tools: Vec<RuntimeToolDescriptor>,
    #[serde(default)]
    effective_mcp_ids: Vec<String>,
    #[serde(default)]
    provider_skills_prompt: Option<String>,
    #[serde(default)]
    plugin_instruction_items: Vec<serde_json::Value>,
    plugin_mcp_bindings: HashMap<String, PluginMcpRuntimeBinding>,
    plugin_local_bindings: HashMap<String, PluginLocalProviderBinding>,
    plugin_tool_component_bindings: HashMap<String, PluginToolComponentRuntimeBinding>,
    plugin_local_tool_component_bindings: HashMap<String, PluginLocalToolComponentBinding>,
    plugin_cloud_tool_component_bindings: HashMap<String, PluginCloudToolComponentBinding>,
    #[serde(default)]
    local_connector_mcp_bindings: HashMap<String, LocalConnectorMcpProviderBinding>,
    expires_at: String,
    expires_at_unix: i64,
}

#[derive(Clone)]
struct SnapshotCipher {
    key: [u8; 32],
}

impl RuntimeSessionStore {
    pub fn memory() -> Self {
        Self {
            backend: Arc::new(RuntimeSessionStoreBackend::Memory(RwLock::new(
                HashMap::new(),
            ))),
        }
    }

    pub async fn connect(
        database_url: &str,
        encryption_secret: &str,
        _external_http_request_timeout: Duration,
        cache_limits: RuntimeSessionCacheLimits,
    ) -> Result<Self, String> {
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|error| format!("connect MCP Management MongoDB failed: {error}"))?;
        let database = client.default_database().ok_or_else(|| {
            "MCP_MANAGEMENT_DATABASE_URL must include a MongoDB database name".to_string()
        })?;
        let collection = database
            .collection::<StoredRuntimeSessionDocument>("mcp_management_runtime_session_snapshots");
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "expires_at": 1 })
                    .options(
                        IndexOptions::builder()
                            .name("runtime_session_expiry_ttl".to_string())
                            .expire_after(Some(Duration::from_secs(0)))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| format!("initialize Runtime Session TTL index failed: {error}"))?;
        Ok(Self {
            backend: Arc::new(RuntimeSessionStoreBackend::Mongo(
                MongoRuntimeSessionStore {
                    collection,
                    cipher: SnapshotCipher::new(encryption_secret)?,
                    cache_limits,
                    cache: RwLock::new(RuntimeSessionCache::default()),
                },
            )),
        })
    }

    pub async fn insert(&self, snapshot: RuntimeSessionSnapshot) -> Result<(), String> {
        match self.backend.as_ref() {
            RuntimeSessionStoreBackend::Memory(sessions) => {
                let now = chrono::Utc::now().timestamp();
                let mut sessions = sessions.write().await;
                sessions.retain(|_, value| value.expires_at_unix > now);
                sessions.insert(snapshot.session_id.clone(), Arc::new(snapshot));
                Ok(())
            }
            RuntimeSessionStoreBackend::Mongo(store) => {
                if snapshot.expires_at_unix <= chrono::Utc::now().timestamp() {
                    return Err("cannot persist an expired Runtime Session Snapshot".to_string());
                }
                let document = store.cipher.encrypt(&snapshot)?;
                let envelope_digest = document.envelope_digest();
                store
                    .collection
                    .replace_one(
                        doc! { "_id": snapshot.session_id.as_str() },
                        document,
                        ReplaceOptions::builder().upsert(true).build(),
                    )
                    .await
                    .map_err(|error| format!("persist Runtime Session Snapshot failed: {error}"))?;
                let mut cache = store.cache.write().await;
                cache_snapshot(&mut cache, envelope_digest, snapshot, store.cache_limits);
                Ok(())
            }
        }
    }

    pub async fn get(
        &self,
        session_id: &str,
    ) -> Result<Option<Arc<RuntimeSessionSnapshot>>, String> {
        match self.backend.as_ref() {
            RuntimeSessionStoreBackend::Memory(sessions) => {
                let now = chrono::Utc::now().timestamp();
                let mut sessions = sessions.write().await;
                sessions.retain(|_, value| value.expires_at_unix > now);
                Ok(sessions.get(session_id).cloned())
            }
            RuntimeSessionStoreBackend::Mongo(store) => {
                let document = store
                    .collection
                    .find_one(doc! { "_id": session_id }, None)
                    .await
                    .map_err(|error| format!("load Runtime Session Snapshot failed: {error}"))?;
                let Some(document) = document else {
                    store.cache.write().await.remove(session_id);
                    return Ok(None);
                };
                if document.expires_at_unix <= chrono::Utc::now().timestamp() {
                    store
                        .collection
                        .delete_one(doc! { "_id": session_id }, None)
                        .await
                        .map_err(|error| {
                            format!("remove expired Runtime Session Snapshot failed: {error}")
                        })?;
                    store.cache.write().await.remove(session_id);
                    return Ok(None);
                }
                let envelope_digest = document.envelope_digest();
                {
                    let mut cache = store.cache.write().await;
                    if let Some(snapshot) = cache.get_if_fresh(
                        session_id,
                        envelope_digest,
                        chrono::Utc::now().timestamp(),
                    ) {
                        return Ok(Some(snapshot));
                    }
                }
                let snapshot = store
                    .cipher
                    .decrypt(document)?;
                let snapshot = Arc::new(snapshot);
                let mut cache = store.cache.write().await;
                cache_snapshot_arc(
                    &mut cache,
                    envelope_digest,
                    snapshot.clone(),
                    store.cache_limits,
                );
                Ok(Some(snapshot))
            }
        }
    }

    pub async fn remove(
        &self,
        session_id: &str,
    ) -> Result<Option<Arc<RuntimeSessionSnapshot>>, String> {
        match self.backend.as_ref() {
            RuntimeSessionStoreBackend::Memory(sessions) => {
                Ok(sessions.write().await.remove(session_id))
            }
            RuntimeSessionStoreBackend::Mongo(store) => {
                let cached = store.cache.write().await.remove(session_id);
                let document = store
                    .collection
                    .find_one_and_delete(doc! { "_id": session_id }, None)
                    .await
                    .map_err(|error| format!("delete Runtime Session Snapshot failed: {error}"))?;
                let Some(document) = document else {
                    return Ok(None);
                };
                if document.expires_at_unix <= chrono::Utc::now().timestamp() {
                    return Ok(None);
                }
                let envelope_digest = document.envelope_digest();
                if let Some(cached) = cached {
                    if cached.envelope_digest == envelope_digest {
                        return Ok(Some(cached.snapshot));
                    }
                }
                store
                    .cipher
                    .decrypt(document)
                    .map(Arc::new)
                    .map(Some)
            }
        }
    }

    pub async fn remove_run_sessions(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<Vec<Arc<RuntimeSessionSnapshot>>, String> {
        let candidates = match self.backend.as_ref() {
            RuntimeSessionStoreBackend::Memory(sessions) => sessions
                .read()
                .await
                .values()
                .filter(|snapshot| {
                    snapshot.owner_user_id == owner_user_id
                        && snapshot.project_id == project_id
                        && snapshot.run_id.as_deref() == Some(run_id)
                })
                .map(|snapshot| snapshot.session_id.clone())
                .collect::<Vec<_>>(),
            RuntimeSessionStoreBackend::Mongo(store) => store
                .collection
                .find(
                    doc! {
                        "execution_scope_hash": execution_scope_hash(owner_user_id, project_id, run_id),
                        "expires_at_unix": { "$gt": chrono::Utc::now().timestamp() },
                    },
                    None,
                )
                .await
                .map_err(|error| format!("find Runtime Sessions for terminal run failed: {error}"))?
                .map_ok(|document| document.session_id)
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| {
                    format!("read Runtime Sessions for terminal run failed: {error}")
                })?,
        };
        let mut removed = Vec::new();
        for session_id in candidates {
            if let Some(snapshot) = self.remove(session_id.as_str()).await? {
                removed.push(snapshot);
            }
        }
        Ok(removed)
    }

    pub async fn stats(&self) -> Result<RuntimeSessionStoreStats, String> {
        let now = chrono::Utc::now().timestamp();
        match self.backend.as_ref() {
            RuntimeSessionStoreBackend::Memory(sessions) => {
                let mut sessions = sessions.write().await;
                sessions.retain(|_, snapshot| snapshot.expires_at_unix > now);
                let snapshot_sizes = sessions
                    .values()
                    .map(|snapshot| estimate_snapshot_cache_bytes(snapshot.as_ref()))
                    .collect::<Vec<_>>();
                let size_stats = summarize_snapshot_sizes(snapshot_sizes.as_slice());
                Ok(RuntimeSessionStoreStats {
                    backend: "memory",
                    active_session_count: sessions.len(),
                    cached_session_count: sessions.len(),
                    cached_total_bytes: size_stats.total_bytes,
                    cached_avg_snapshot_bytes: size_stats.avg_bytes,
                    cached_p95_snapshot_bytes: size_stats.p95_bytes,
                    cache_entry_limit: None,
                    cache_byte_limit: None,
                    cache_hits_total: 0,
                    cache_misses_total: 0,
                    cache_capacity_evictions_total: 0,
                    cache_expired_evictions_total: 0,
                    cache_oversized_rejections_total: 0,
                })
            }
            RuntimeSessionStoreBackend::Mongo(store) => {
                let active_session_count = store
                    .collection
                    .count_documents(doc! { "expires_at_unix": { "$gt": now } }, None)
                    .await
                    .map_err(|error| format!("count active Runtime Sessions failed: {error}"))
                    .map(saturating_u64_to_usize)?;
                let mut cache = store.cache.write().await;
                cache.retain_unexpired(now);
                let snapshot_sizes = cache
                    .entries
                    .values()
                    .map(|entry| entry.approx_size_bytes)
                    .collect::<Vec<_>>();
                let size_stats = summarize_snapshot_sizes(snapshot_sizes.as_slice());
                Ok(RuntimeSessionStoreStats {
                    backend: "mongo",
                    active_session_count,
                    cached_session_count: cache.entries.len(),
                    cached_total_bytes: cache.total_bytes,
                    cached_avg_snapshot_bytes: size_stats.avg_bytes,
                    cached_p95_snapshot_bytes: size_stats.p95_bytes,
                    cache_entry_limit: Some(store.cache_limits.max_entries),
                    cache_byte_limit: Some(store.cache_limits.max_bytes),
                    cache_hits_total: cache.hits_total,
                    cache_misses_total: cache.misses_total,
                    cache_capacity_evictions_total: cache.capacity_evictions_total,
                    cache_expired_evictions_total: cache.expired_evictions_total,
                    cache_oversized_rejections_total: cache.oversized_rejections_total,
                })
            }
        }
    }
}

impl StoredRuntimeSessionDocument {
    fn envelope_digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.schema_version.to_be_bytes());
        hasher.update(self.expires_at_unix.to_be_bytes());
        if let Some(scope_hash) = self.execution_scope_hash.as_deref() {
            hasher.update(scope_hash.as_bytes());
        }
        hasher.update(self.nonce.bytes.as_slice());
        hasher.update(self.encrypted_snapshot.bytes.as_slice());
        hasher.finalize().into()
    }
}

impl SnapshotCipher {
    fn new(secret: &str) -> Result<Self, String> {
        let secret = secret.trim();
        if secret.is_empty() {
            return Err("Runtime Session encryption secret cannot be empty".to_string());
        }
        let digest = Sha256::digest(secret.as_bytes());
        let mut key = [0_u8; 32];
        key.copy_from_slice(digest.as_slice());
        Ok(Self { key })
    }

    fn encrypt(
        &self,
        snapshot: &RuntimeSessionSnapshot,
    ) -> Result<StoredRuntimeSessionDocument, String> {
        let persisted = PersistedRuntimeSessionSnapshot::try_from(snapshot)?;
        let plain = serde_json::to_vec(&persisted)
            .map_err(|error| format!("serialize Runtime Session Snapshot failed: {error}"))?;
        if plain.len() > MAX_PERSISTED_SNAPSHOT_BYTES {
            return Err(format!(
                "Runtime Session Snapshot exceeds the supported encrypted size: {} bytes > {} bytes",
                plain.len(),
                MAX_PERSISTED_SNAPSHOT_BYTES
            ));
        }
        let mut nonce = [0_u8; SNAPSHOT_NONCE_BYTES];
        rand::fill(&mut nonce);
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|error| format!("initialize Runtime Session cipher failed: {error}"))?;
        let nonce_ref = Nonce::try_from(nonce.as_slice())
            .map_err(|error| format!("initialize Runtime Session nonce failed: {error}"))?;
        let encrypted_snapshot = cipher
            .encrypt(
                &nonce_ref,
                Payload {
                    msg: plain.as_slice(),
                    aad: snapshot.session_id.as_bytes(),
                },
            )
            .map_err(|error| format!("encrypt Runtime Session Snapshot failed: {error}"))?;
        Ok(StoredRuntimeSessionDocument {
            session_id: snapshot.session_id.clone(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            expires_at: DateTime::from_millis(snapshot.expires_at_unix.saturating_mul(1_000)),
            expires_at_unix: snapshot.expires_at_unix,
            execution_scope_hash: snapshot.run_id.as_deref().map(|run_id| {
                execution_scope_hash(
                    snapshot.owner_user_id.as_str(),
                    snapshot.project_id.as_str(),
                    run_id,
                )
            }),
            nonce: Binary {
                subtype: BinarySubtype::Generic,
                bytes: nonce.to_vec(),
            },
            encrypted_snapshot: Binary {
                subtype: BinarySubtype::Generic,
                bytes: encrypted_snapshot,
            },
        })
    }

    fn decrypt(&self, document: StoredRuntimeSessionDocument) -> Result<RuntimeSessionSnapshot, String> {
        if document.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported Runtime Session Snapshot schema version: {}",
                document.schema_version
            ));
        }
        if document.nonce.bytes.len() != SNAPSHOT_NONCE_BYTES {
            return Err("Runtime Session Snapshot nonce has an invalid size".to_string());
        }
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|error| format!("initialize Runtime Session cipher failed: {error}"))?;
        let nonce_ref = Nonce::try_from(document.nonce.bytes.as_slice())
            .map_err(|error| format!("initialize Runtime Session nonce failed: {error}"))?;
        let plain = cipher
            .decrypt(
                &nonce_ref,
                Payload {
                    msg: document.encrypted_snapshot.bytes.as_slice(),
                    aad: document.session_id.as_bytes(),
                },
            )
            .map_err(|_| {
                "decrypt Runtime Session Snapshot failed: key mismatch or corrupted data"
                    .to_string()
            })?;
        let persisted = serde_json::from_slice::<PersistedRuntimeSessionSnapshot>(&plain)
            .map_err(|error| format!("decode Runtime Session Snapshot failed: {error}"))?;
        if persisted.session_id != document.session_id
            || persisted.expires_at_unix != document.expires_at_unix
        {
            return Err(
                "Runtime Session Snapshot metadata does not match its envelope".to_string(),
            );
        }
        persisted.into_runtime()
    }
}

fn execution_scope_hash(owner_user_id: &str, project_id: &str, run_id: &str) -> String {
    let identity = format!(
        "{}\u{1f}{}\u{1f}{}",
        owner_user_id.trim(),
        project_id.trim(),
        run_id.trim()
    );
    hex::encode(Sha256::digest(identity.as_bytes()))
}

impl TryFrom<&RuntimeSessionSnapshot> for PersistedRuntimeSessionSnapshot {
    type Error = String;

    fn try_from(snapshot: &RuntimeSessionSnapshot) -> Result<Self, Self::Error> {
        Ok(Self {
            session_id: snapshot.session_id.clone(),
            caller_service: snapshot.caller_service.clone(),
            trace_id: snapshot.trace_id.clone(),
            tenant_id: snapshot.tenant_id.clone(),
            owner_user_id: snapshot.owner_user_id.clone(),
            owner_role: snapshot.owner_role.clone(),
            agent_key: snapshot.agent_key.clone(),
            task_profile: snapshot.task_profile.clone(),
            project_id: snapshot.project_id.clone(),
            device_id: snapshot.device_id.clone(),
            run_id: snapshot.run_id.clone(),
            execution_group_id: snapshot.execution_group_id.clone(),
            execution_scope_generation: snapshot.execution_scope_generation,
            turn_id: snapshot.turn_id.clone(),
            task_id: snapshot.task_id.clone(),
            source_session_id: snapshot.source_session_id.clone(),
            source_user_message_id: snapshot.source_user_message_id.clone(),
            contact_agent_id: snapshot.contact_agent_id.clone(),
            default_model_config_id: snapshot.default_model_config_id.clone(),
            tool_result_max_chars: snapshot.tool_result_max_chars,
            expected_project_task_ids: snapshot.expected_project_task_ids.clone(),
            workspace_route: snapshot.workspace_route.clone(),
            project_context: snapshot.project_context.clone(),
            policy_revision: snapshot.policy_revision.clone(),
            route_revision: snapshot.route_revision.clone(),
            routes: snapshot.routes.clone(),
            tools: snapshot.tools.clone(),
            effective_mcp_ids: snapshot.effective_mcp_ids.clone(),
            provider_skills_prompt: snapshot.provider_skills_prompt.clone(),
            plugin_instruction_items: snapshot.plugin_instruction_items.clone(),
            plugin_mcp_bindings: snapshot.plugin_mcp_bindings.clone(),
            plugin_local_bindings: snapshot.plugin_local_bindings.clone(),
            plugin_tool_component_bindings: snapshot.plugin_tool_component_bindings.clone(),
            plugin_local_tool_component_bindings: snapshot
                .plugin_local_tool_component_bindings
                .clone(),
            plugin_cloud_tool_component_bindings: snapshot
                .plugin_cloud_tool_component_bindings
                .clone(),
            local_connector_mcp_bindings: snapshot.local_connector_mcp_bindings.clone(),
            expires_at: snapshot.expires_at.clone(),
            expires_at_unix: snapshot.expires_at_unix,
        })
    }
}

impl PersistedRuntimeSessionSnapshot {
    fn into_runtime(self) -> Result<RuntimeSessionSnapshot, String> {
        Ok(RuntimeSessionSnapshot {
            session_id: self.session_id,
            caller_service: self.caller_service,
            trace_id: self.trace_id,
            tenant_id: self.tenant_id,
            owner_user_id: self.owner_user_id,
            owner_role: self.owner_role,
            agent_key: self.agent_key,
            task_profile: self.task_profile,
            project_id: self.project_id,
            device_id: self.device_id,
            run_id: self.run_id,
            execution_group_id: self.execution_group_id,
            execution_scope_generation: self.execution_scope_generation,
            turn_id: self.turn_id,
            task_id: self.task_id,
            source_session_id: self.source_session_id,
            source_user_message_id: self.source_user_message_id,
            contact_agent_id: self.contact_agent_id,
            default_model_config_id: self.default_model_config_id,
            tool_result_max_chars: self.tool_result_max_chars,
            expected_project_task_ids: self.expected_project_task_ids,
            workspace_route: self.workspace_route,
            project_context: self.project_context,
            policy_revision: self.policy_revision,
            route_revision: self.route_revision,
            routes: self.routes,
            tools: self.tools,
            effective_mcp_ids: self.effective_mcp_ids,
            provider_skills_prompt: self.provider_skills_prompt,
            plugin_instruction_items: self.plugin_instruction_items,
            plugin_mcp_bindings: self.plugin_mcp_bindings,
            plugin_local_bindings: self.plugin_local_bindings,
            plugin_tool_component_bindings: self.plugin_tool_component_bindings,
            plugin_local_tool_component_bindings: self.plugin_local_tool_component_bindings,
            plugin_cloud_tool_component_bindings: self.plugin_cloud_tool_component_bindings,
            local_connector_mcp_bindings: self.local_connector_mcp_bindings,
            expires_at: self.expires_at,
            expires_at_unix: self.expires_at_unix,
        })
    }
}

#[cfg(test)]
mod tests;
