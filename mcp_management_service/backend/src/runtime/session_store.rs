// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use chatos_mcp_management_sdk::{
    ProjectExecutionContext, ResolvedMcpRoute, RuntimeSessionRoutesResponse, RuntimeToolDescriptor,
    SandboxExecutionTarget,
};
use chatos_plugin_management_sdk::PluginMcpCloudRuntimeBundle;
use mongodb::bson::{doc, spec::BinarySubtype, Binary, DateTime};
use mongodb::options::{IndexOptions, ReplaceOptions};
use mongodb::{Client, Collection, IndexModel};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::providers::{
    build_pinned_external_http_client, external_http_header_is_managed_or_unsafe,
};
use crate::runtime::{
    PluginCloudToolComponentBinding, PluginLocalProviderBinding, PluginLocalToolComponentBinding,
    PluginMcpRuntimeBinding, PluginToolComponentRuntimeBinding,
};

const SNAPSHOT_SCHEMA_VERSION: i32 = 3;
const SNAPSHOT_NONCE_BYTES: usize = 12;
const MAX_PERSISTED_HEADERS: usize = 64;
const MAX_PERSISTED_HEADER_BYTES: usize = 32 * 1024;
const MAX_PERSISTED_TOOL_POLICY_ITEMS: usize = 512;
const MAX_PERSISTED_TOOL_NAME_BYTES: usize = 256;
const MAX_PERSISTED_SNAPSHOT_BYTES: usize = 12 * 1024 * 1024;
const MAX_RUNTIME_SESSION_CACHE_ENTRIES: usize = 2_048;

#[derive(Clone)]
pub struct ExternalHttpProviderBinding {
    pub provider_ref: String,
    pub endpoint: reqwest::Url,
    pub headers: reqwest::header::HeaderMap,
    pub http: reqwest::Client,
    pub resolved_addresses: Vec<SocketAddr>,
    pub allow_writes: bool,
    pub allowed_tool_names: HashSet<String>,
    pub blocked_tool_names: HashSet<String>,
}

impl ExternalHttpProviderBinding {
    pub fn allows_tool(&self, tool_name: &str) -> bool {
        let tool_name = tool_name.trim();
        !tool_name.is_empty()
            && (self.allowed_tool_names.is_empty() || self.allowed_tool_names.contains(tool_name))
            && !self.blocked_tool_names.contains(tool_name)
    }
}

impl fmt::Debug for ExternalHttpProviderBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExternalHttpProviderBinding")
            .field("provider_ref", &self.provider_ref)
            .field("endpoint", &"[redacted]")
            .field("headers", &"[redacted]")
            .field("resolved_addresses", &"[redacted]")
            .field("allow_writes", &self.allow_writes)
            .field("allowed_tool_names", &self.allowed_tool_names)
            .field("blocked_tool_names", &self.blocked_tool_names)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CloudStdioProviderBinding {
    pub provider_ref: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub plugin_artifact: Option<PluginMcpCloudRuntimeBundle>,
    pub allow_writes: bool,
    pub allowed_tool_names: HashSet<String>,
    pub blocked_tool_names: HashSet<String>,
}

impl CloudStdioProviderBinding {
    pub fn allows_tool(&self, tool_name: &str) -> bool {
        let tool_name = tool_name.trim();
        !tool_name.is_empty()
            && (self.allowed_tool_names.is_empty() || self.allowed_tool_names.contains(tool_name))
            && !self.blocked_tool_names.contains(tool_name)
    }
}

impl fmt::Debug for CloudStdioProviderBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CloudStdioProviderBinding")
            .field("provider_ref", &self.provider_ref)
            .field("command", &"[redacted]")
            .field("args", &"[redacted]")
            .field("env", &"[redacted]")
            .field("cwd", &"[redacted]")
            .field(
                "plugin_artifact",
                &self.plugin_artifact.as_ref().map(|_| "[bound]"),
            )
            .field("allow_writes", &self.allow_writes)
            .field("allowed_tool_names", &self.allowed_tool_names)
            .field("blocked_tool_names", &self.blocked_tool_names)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeSessionSnapshot {
    pub session_id: String,
    pub caller_service: String,
    pub owner_user_id: String,
    pub agent_key: String,
    pub project_id: String,
    pub run_id: Option<String>,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub default_model_config_id: Option<String>,
    pub expected_project_task_ids: Vec<String>,
    pub sandbox_target: Option<SandboxExecutionTarget>,
    pub project_context: ProjectExecutionContext,
    pub policy_revision: String,
    pub route_revision: String,
    pub routes: Vec<ResolvedMcpRoute>,
    pub tools: Vec<RuntimeToolDescriptor>,
    pub plugin_mcp_bindings: HashMap<String, PluginMcpRuntimeBinding>,
    pub plugin_local_bindings: HashMap<String, PluginLocalProviderBinding>,
    pub plugin_tool_component_bindings: HashMap<String, PluginToolComponentRuntimeBinding>,
    pub plugin_local_tool_component_bindings: HashMap<String, PluginLocalToolComponentBinding>,
    pub plugin_cloud_tool_component_bindings: HashMap<String, PluginCloudToolComponentBinding>,
    pub external_http_bindings: HashMap<String, ExternalHttpProviderBinding>,
    pub cloud_stdio_bindings: HashMap<String, CloudStdioProviderBinding>,
    pub expires_at: String,
    pub expires_at_unix: i64,
}

impl RuntimeSessionSnapshot {
    pub fn routes_response(&self) -> RuntimeSessionRoutesResponse {
        RuntimeSessionRoutesResponse {
            session_id: self.session_id.clone(),
            owner_user_id: self.owner_user_id.clone(),
            agent_key: self.agent_key.clone(),
            project_id: self.project_id.clone(),
            run_id: self.run_id.clone(),
            policy_revision: self.policy_revision.clone(),
            route_revision: self.route_revision.clone(),
            expires_at: self.expires_at.clone(),
            routes: self.routes.clone(),
            tools: self.tools.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RuntimeSessionStore {
    backend: Arc<RuntimeSessionStoreBackend>,
}

enum RuntimeSessionStoreBackend {
    Memory(RwLock<HashMap<String, RuntimeSessionSnapshot>>),
    Mongo(MongoRuntimeSessionStore),
}

struct MongoRuntimeSessionStore {
    collection: Collection<StoredRuntimeSessionDocument>,
    cipher: SnapshotCipher,
    external_http_request_timeout: Duration,
    cache: RwLock<HashMap<String, CachedRuntimeSessionSnapshot>>,
}

#[derive(Clone)]
struct CachedRuntimeSessionSnapshot {
    envelope_digest: [u8; 32],
    snapshot: RuntimeSessionSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRuntimeSessionDocument {
    #[serde(rename = "_id")]
    session_id: String,
    schema_version: i32,
    expires_at: DateTime,
    expires_at_unix: i64,
    nonce: Binary,
    encrypted_snapshot: Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedRuntimeSessionSnapshot {
    session_id: String,
    caller_service: String,
    owner_user_id: String,
    agent_key: String,
    project_id: String,
    run_id: Option<String>,
    turn_id: Option<String>,
    task_id: Option<String>,
    source_session_id: Option<String>,
    source_user_message_id: Option<String>,
    contact_agent_id: Option<String>,
    default_model_config_id: Option<String>,
    expected_project_task_ids: Vec<String>,
    sandbox_target: Option<SandboxExecutionTarget>,
    project_context: ProjectExecutionContext,
    policy_revision: String,
    route_revision: String,
    routes: Vec<ResolvedMcpRoute>,
    tools: Vec<RuntimeToolDescriptor>,
    plugin_mcp_bindings: HashMap<String, PluginMcpRuntimeBinding>,
    plugin_local_bindings: HashMap<String, PluginLocalProviderBinding>,
    plugin_tool_component_bindings: HashMap<String, PluginToolComponentRuntimeBinding>,
    plugin_local_tool_component_bindings: HashMap<String, PluginLocalToolComponentBinding>,
    plugin_cloud_tool_component_bindings: HashMap<String, PluginCloudToolComponentBinding>,
    external_http_bindings: HashMap<String, PersistedExternalHttpProviderBinding>,
    cloud_stdio_bindings: HashMap<String, CloudStdioProviderBinding>,
    expires_at: String,
    expires_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedExternalHttpProviderBinding {
    provider_ref: String,
    endpoint: String,
    headers: Vec<PersistedHeader>,
    resolved_addresses: Vec<String>,
    allow_writes: bool,
    allowed_tool_names: HashSet<String>,
    blocked_tool_names: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedHeader {
    name: String,
    value: Vec<u8>,
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
        external_http_request_timeout: Duration,
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
                    external_http_request_timeout,
                    cache: RwLock::new(HashMap::new()),
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
                sessions.insert(snapshot.session_id.clone(), snapshot);
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
                cache_snapshot(&mut cache, envelope_digest, snapshot);
                Ok(())
            }
        }
    }

    pub async fn get(&self, session_id: &str) -> Result<Option<RuntimeSessionSnapshot>, String> {
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
                if let Some(cached) = store.cache.read().await.get(session_id).cloned() {
                    if cached.envelope_digest == envelope_digest {
                        return Ok(Some(cached.snapshot));
                    }
                }
                let snapshot = store
                    .cipher
                    .decrypt(document, store.external_http_request_timeout)?;
                let mut cache = store.cache.write().await;
                cache_snapshot(&mut cache, envelope_digest, snapshot.clone());
                Ok(Some(snapshot))
            }
        }
    }

    pub async fn remove(&self, session_id: &str) -> Result<Option<RuntimeSessionSnapshot>, String> {
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
                    .decrypt(document, store.external_http_request_timeout)
                    .map(Some)
            }
        }
    }
}

impl StoredRuntimeSessionDocument {
    fn envelope_digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.schema_version.to_be_bytes());
        hasher.update(self.expires_at_unix.to_be_bytes());
        hasher.update(self.nonce.bytes.as_slice());
        hasher.update(self.encrypted_snapshot.bytes.as_slice());
        hasher.finalize().into()
    }
}

fn cache_snapshot(
    cache: &mut HashMap<String, CachedRuntimeSessionSnapshot>,
    envelope_digest: [u8; 32],
    snapshot: RuntimeSessionSnapshot,
) {
    let now = chrono::Utc::now().timestamp();
    cache.retain(|_, cached| cached.snapshot.expires_at_unix > now);
    if cache.len() >= MAX_RUNTIME_SESSION_CACHE_ENTRIES
        && !cache.contains_key(snapshot.session_id.as_str())
    {
        cache.clear();
    }
    cache.insert(
        snapshot.session_id.clone(),
        CachedRuntimeSessionSnapshot {
            envelope_digest,
            snapshot,
        },
    );
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

    fn decrypt(
        &self,
        document: StoredRuntimeSessionDocument,
        external_http_request_timeout: Duration,
    ) -> Result<RuntimeSessionSnapshot, String> {
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
        persisted.into_runtime(external_http_request_timeout)
    }
}

impl TryFrom<&RuntimeSessionSnapshot> for PersistedRuntimeSessionSnapshot {
    type Error = String;

    fn try_from(snapshot: &RuntimeSessionSnapshot) -> Result<Self, Self::Error> {
        let external_http_bindings = snapshot
            .external_http_bindings
            .iter()
            .map(|(resource_id, binding)| {
                persist_external_http_binding(binding)
                    .map(|persisted| (resource_id.clone(), persisted))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(Self {
            session_id: snapshot.session_id.clone(),
            caller_service: snapshot.caller_service.clone(),
            owner_user_id: snapshot.owner_user_id.clone(),
            agent_key: snapshot.agent_key.clone(),
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            turn_id: snapshot.turn_id.clone(),
            task_id: snapshot.task_id.clone(),
            source_session_id: snapshot.source_session_id.clone(),
            source_user_message_id: snapshot.source_user_message_id.clone(),
            contact_agent_id: snapshot.contact_agent_id.clone(),
            default_model_config_id: snapshot.default_model_config_id.clone(),
            expected_project_task_ids: snapshot.expected_project_task_ids.clone(),
            sandbox_target: snapshot.sandbox_target.clone(),
            project_context: snapshot.project_context.clone(),
            policy_revision: snapshot.policy_revision.clone(),
            route_revision: snapshot.route_revision.clone(),
            routes: snapshot.routes.clone(),
            tools: snapshot.tools.clone(),
            plugin_mcp_bindings: snapshot.plugin_mcp_bindings.clone(),
            plugin_local_bindings: snapshot.plugin_local_bindings.clone(),
            plugin_tool_component_bindings: snapshot.plugin_tool_component_bindings.clone(),
            plugin_local_tool_component_bindings: snapshot
                .plugin_local_tool_component_bindings
                .clone(),
            plugin_cloud_tool_component_bindings: snapshot
                .plugin_cloud_tool_component_bindings
                .clone(),
            external_http_bindings,
            cloud_stdio_bindings: snapshot.cloud_stdio_bindings.clone(),
            expires_at: snapshot.expires_at.clone(),
            expires_at_unix: snapshot.expires_at_unix,
        })
    }
}

impl PersistedRuntimeSessionSnapshot {
    fn into_runtime(
        self,
        external_http_request_timeout: Duration,
    ) -> Result<RuntimeSessionSnapshot, String> {
        let external_http_bindings = self
            .external_http_bindings
            .into_iter()
            .map(|(resource_id, binding)| {
                restore_external_http_binding(binding, external_http_request_timeout)
                    .map(|restored| (resource_id, restored))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(RuntimeSessionSnapshot {
            session_id: self.session_id,
            caller_service: self.caller_service,
            owner_user_id: self.owner_user_id,
            agent_key: self.agent_key,
            project_id: self.project_id,
            run_id: self.run_id,
            turn_id: self.turn_id,
            task_id: self.task_id,
            source_session_id: self.source_session_id,
            source_user_message_id: self.source_user_message_id,
            contact_agent_id: self.contact_agent_id,
            default_model_config_id: self.default_model_config_id,
            expected_project_task_ids: self.expected_project_task_ids,
            sandbox_target: self.sandbox_target,
            project_context: self.project_context,
            policy_revision: self.policy_revision,
            route_revision: self.route_revision,
            routes: self.routes,
            tools: self.tools,
            plugin_mcp_bindings: self.plugin_mcp_bindings,
            plugin_local_bindings: self.plugin_local_bindings,
            plugin_tool_component_bindings: self.plugin_tool_component_bindings,
            plugin_local_tool_component_bindings: self.plugin_local_tool_component_bindings,
            plugin_cloud_tool_component_bindings: self.plugin_cloud_tool_component_bindings,
            external_http_bindings,
            cloud_stdio_bindings: self.cloud_stdio_bindings,
            expires_at: self.expires_at,
            expires_at_unix: self.expires_at_unix,
        })
    }
}

fn persist_external_http_binding(
    binding: &ExternalHttpProviderBinding,
) -> Result<PersistedExternalHttpProviderBinding, String> {
    let headers = binding
        .headers
        .iter()
        .map(|(name, value)| PersistedHeader {
            name: name.as_str().to_string(),
            value: value.as_bytes().to_vec(),
        })
        .collect::<Vec<_>>();
    validate_persisted_headers(headers.as_slice())?;
    validate_persisted_tool_names(&binding.allowed_tool_names, "allowed_tool_names")?;
    validate_persisted_tool_names(&binding.blocked_tool_names, "blocked_tool_names")?;
    Ok(PersistedExternalHttpProviderBinding {
        provider_ref: binding.provider_ref.clone(),
        endpoint: binding.endpoint.as_str().to_string(),
        headers,
        resolved_addresses: binding
            .resolved_addresses
            .iter()
            .map(ToString::to_string)
            .collect(),
        allow_writes: binding.allow_writes,
        allowed_tool_names: binding.allowed_tool_names.clone(),
        blocked_tool_names: binding.blocked_tool_names.clone(),
    })
}

fn restore_external_http_binding(
    persisted: PersistedExternalHttpProviderBinding,
    request_timeout: Duration,
) -> Result<ExternalHttpProviderBinding, String> {
    if persisted.provider_ref.trim().is_empty() {
        return Err("persisted External HTTP Provider reference is empty".to_string());
    }
    validate_persisted_headers(persisted.headers.as_slice())?;
    validate_persisted_tool_names(&persisted.allowed_tool_names, "allowed_tool_names")?;
    validate_persisted_tool_names(&persisted.blocked_tool_names, "blocked_tool_names")?;
    let endpoint = reqwest::Url::parse(persisted.endpoint.trim())
        .map_err(|_| "persisted External HTTP endpoint is invalid".to_string())?;
    let resolved_addresses = persisted
        .resolved_addresses
        .iter()
        .map(|value| {
            value
                .parse::<SocketAddr>()
                .map_err(|_| "persisted External HTTP address is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let http = build_pinned_external_http_client(
        &endpoint,
        resolved_addresses.as_slice(),
        request_timeout,
    )?;
    let mut headers = HeaderMap::new();
    for persisted_header in persisted.headers {
        let name = HeaderName::from_bytes(persisted_header.name.as_bytes())
            .map_err(|_| "persisted External HTTP header name is invalid".to_string())?;
        if external_http_header_is_managed_or_unsafe(&name) {
            return Err("persisted External HTTP header is managed or unsafe".to_string());
        }
        let mut value = HeaderValue::from_bytes(persisted_header.value.as_slice())
            .map_err(|_| "persisted External HTTP header value is invalid".to_string())?;
        value.set_sensitive(true);
        headers.append(name, value);
    }
    Ok(ExternalHttpProviderBinding {
        provider_ref: persisted.provider_ref,
        endpoint,
        headers,
        http,
        resolved_addresses,
        allow_writes: persisted.allow_writes,
        allowed_tool_names: persisted.allowed_tool_names,
        blocked_tool_names: persisted.blocked_tool_names,
    })
}

fn validate_persisted_headers(headers: &[PersistedHeader]) -> Result<(), String> {
    if headers.len() > MAX_PERSISTED_HEADERS {
        return Err("persisted External HTTP headers exceed the supported limit".to_string());
    }
    let bytes = headers.iter().fold(0_usize, |total, header| {
        total
            .saturating_add(header.name.len())
            .saturating_add(header.value.len())
    });
    if bytes > MAX_PERSISTED_HEADER_BYTES {
        return Err("persisted External HTTP headers exceed the supported size".to_string());
    }
    Ok(())
}

fn validate_persisted_tool_names(values: &HashSet<String>, field: &str) -> Result<(), String> {
    if values.len() > MAX_PERSISTED_TOOL_POLICY_ITEMS
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > MAX_PERSISTED_TOOL_NAME_BYTES)
    {
        return Err(format!("persisted External HTTP {field} is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chatos_mcp_management_sdk::{
        ExecutionPlane, ProjectExecutionContext, SandboxProviderKind, WorkspaceProviderKind,
    };
    use chatos_plugin_management_sdk::{PluginExecutionHost, PluginMcpServer};

    use super::*;

    fn plugin_runtime_binding() -> PluginMcpRuntimeBinding {
        PluginMcpRuntimeBinding {
            provider_ref: format!("plugin-binding:{}", "b".repeat(64)),
            resource_id: "plugin-mcp-1".to_string(),
            plugin_id: "private-plugin-1".to_string(),
            release_id: "private-release-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: "a".repeat(64),
            normalized_manifest_sha256: "b".repeat(64),
            component_key: "workspace".to_string(),
            component_content_sha256: "c".repeat(64),
            declared_execution_host: PluginExecutionHost::Local,
            installation_device_id: Some("device-private-1".to_string()),
            permission_snapshot: vec!["workspace.read".to_string()],
            auth_connection_ids: vec!["oauth-private-reference".to_string()],
            runtime: PluginMcpServer::Http {
                component_key: "workspace".to_string(),
                url: "https://plugin-private.example.com/mcp".to_string(),
                headers: Default::default(),
                oauth_resource: None,
                connect_timeout_ms: None,
            },
            server_key: None,
            tool_allowlist: vec!["read_file".to_string()],
            tool_blocklist: Vec::new(),
            required: true,
            allow_writes: false,
        }
    }

    fn snapshot(session_id: &str) -> RuntimeSessionSnapshot {
        let expires_at_unix = chrono::Utc::now().timestamp() + 300;
        let plugin_runtime = plugin_runtime_binding();
        let mut headers = HeaderMap::new();
        let mut authorization = HeaderValue::from_static("Bearer shared-store-secret");
        authorization.set_sensitive(true);
        headers.insert("authorization", authorization);
        RuntimeSessionSnapshot {
            session_id: session_id.to_string(),
            caller_service: "task-runner".to_string(),
            owner_user_id: "owner-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            project_id: "project-1".to_string(),
            run_id: Some("run-1".to_string()),
            turn_id: Some("turn-1".to_string()),
            task_id: Some("task-1".to_string()),
            source_session_id: Some("conversation-1".to_string()),
            source_user_message_id: Some("message-1".to_string()),
            contact_agent_id: Some("contact-1".to_string()),
            default_model_config_id: Some("model-1".to_string()),
            expected_project_task_ids: vec!["task-1".to_string()],
            sandbox_target: Some(SandboxExecutionTarget {
                sandbox_id: "sandbox-1".to_string(),
                lease_id: "lease-1".to_string(),
                is_environment: false,
                service_id: None,
            }),
            project_context: ProjectExecutionContext {
                project_id: "project-1".to_string(),
                owner_user_id: "owner-1".to_string(),
                execution_plane: ExecutionPlane::Cloud,
                workspace_provider: WorkspaceProviderKind::CloudSandbox,
                workspace: None,
                sandbox_provider: SandboxProviderKind::Cloud,
                sandbox_pairing_id: None,
                source_type: Some("cloud".to_string()),
                revision: "project-revision-1".to_string(),
            },
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            routes: Vec::new(),
            tools: Vec::new(),
            plugin_mcp_bindings: HashMap::from([(
                plugin_runtime.resource_id.clone(),
                plugin_runtime.clone(),
            )]),
            plugin_local_bindings: HashMap::from([(
                plugin_runtime.resource_id.clone(),
                PluginLocalProviderBinding {
                    runtime: plugin_runtime,
                    run_id: session_id.to_string(),
                    device_id: "device-private-1".to_string(),
                    workspace_id: "workspace-private-1".to_string(),
                    adapter_session_id: "adapter-private-1".to_string(),
                    operation: "mcp_tools_call".to_string(),
                    session_sha256: "d".repeat(64),
                    tool_snapshot_sha256: "e".repeat(64),
                    tools: vec![serde_json::json!({
                        "name": "read_file",
                        "inputSchema": {"type": "object"}
                    })],
                    oauth_connection_id: Some("oauth-private-reference".to_string()),
                    expires_at_unix,
                },
            )]),
            plugin_tool_component_bindings: Default::default(),
            plugin_local_tool_component_bindings: Default::default(),
            plugin_cloud_tool_component_bindings: Default::default(),
            external_http_bindings: HashMap::from([(
                "external-1".to_string(),
                ExternalHttpProviderBinding {
                    provider_ref: "mcp-resource:external-1".to_string(),
                    endpoint: reqwest::Url::parse("https://mcp.example.com/rpc").unwrap(),
                    headers,
                    http: reqwest::Client::new(),
                    resolved_addresses: vec!["8.8.8.8:443".parse().unwrap()],
                    allow_writes: false,
                    allowed_tool_names: HashSet::from(["search".to_string()]),
                    blocked_tool_names: HashSet::from(["delete".to_string()]),
                },
            )]),
            cloud_stdio_bindings: HashMap::from([(
                "stdio-1".to_string(),
                CloudStdioProviderBinding {
                    provider_ref: "sandbox:sandbox-1/lease:lease-1".to_string(),
                    command: "node".to_string(),
                    args: vec!["server.js".to_string()],
                    env: BTreeMap::from([(
                        "PLUGIN_TOKEN".to_string(),
                        "stdio-shared-store-secret".to_string(),
                    )]),
                    cwd: Some("/workspace/plugin".to_string()),
                    plugin_artifact: None,
                    allow_writes: true,
                    allowed_tool_names: HashSet::new(),
                    blocked_tool_names: HashSet::new(),
                },
            )]),
            expires_at: chrono::DateTime::from_timestamp(expires_at_unix, 0)
                .unwrap()
                .to_rfc3339(),
            expires_at_unix,
        }
    }

    #[tokio::test]
    async fn memory_store_preserves_insert_get_and_atomic_remove_semantics() {
        let store = RuntimeSessionStore::memory();
        store.insert(snapshot("memory-session")).await.unwrap();
        assert_eq!(
            store
                .get("memory-session")
                .await
                .unwrap()
                .unwrap()
                .owner_user_id,
            "owner-1"
        );
        assert!(store.remove("memory-session").await.unwrap().is_some());
        assert!(store.get("memory-session").await.unwrap().is_none());
    }

    #[test]
    fn encrypted_snapshot_roundtrip_preserves_private_bindings_without_plaintext_at_rest() {
        let cipher = SnapshotCipher::new("shared-session-encryption-secret").unwrap();
        let snapshot = snapshot("encrypted-session");
        let document = cipher.encrypt(&snapshot).unwrap();
        let encoded = mongodb::bson::to_vec(&document).unwrap();
        for secret in [
            b"shared-store-secret".as_slice(),
            b"stdio-shared-store-secret".as_slice(),
            b"/workspace/plugin".as_slice(),
            b"oauth-private-reference".as_slice(),
            b"plugin-private.example.com".as_slice(),
        ] {
            assert!(!encoded.windows(secret.len()).any(|window| window == secret));
        }

        let restored = cipher.decrypt(document, Duration::from_secs(60)).unwrap();
        let external = restored.external_http_bindings.get("external-1").unwrap();
        assert_eq!(
            external
                .headers
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer shared-store-secret"
        );
        assert_eq!(external.resolved_addresses[0].to_string(), "8.8.8.8:443");
        assert_eq!(
            restored.cloud_stdio_bindings["stdio-1"].env["PLUGIN_TOKEN"],
            "stdio-shared-store-secret"
        );
        assert_eq!(
            restored.plugin_mcp_bindings["plugin-mcp-1"].release_id,
            "private-release-1"
        );
        assert_eq!(
            restored.plugin_local_bindings["plugin-mcp-1"].adapter_session_id,
            "adapter-private-1"
        );
    }

    #[test]
    fn encrypted_snapshot_rejects_envelope_identity_tampering_and_wrong_keys() {
        let cipher = SnapshotCipher::new("shared-session-encryption-secret").unwrap();
        let mut document = cipher.encrypt(&snapshot("bound-session")).unwrap();
        document.session_id = "attacker-session".to_string();
        assert!(cipher
            .decrypt(document, Duration::from_secs(60))
            .unwrap_err()
            .contains("key mismatch or corrupted data"));

        let document = cipher.encrypt(&snapshot("wrong-key-session")).unwrap();
        let wrong_cipher = SnapshotCipher::new("another-encryption-secret").unwrap();
        assert!(wrong_cipher
            .decrypt(document, Duration::from_secs(60))
            .is_err());
    }

    #[test]
    fn restored_external_http_binding_revalidates_pinned_public_addresses() {
        let binding = PersistedExternalHttpProviderBinding {
            provider_ref: "mcp-resource:external-1".to_string(),
            endpoint: "https://mcp.example.com/rpc".to_string(),
            headers: Vec::new(),
            resolved_addresses: vec!["127.0.0.1:443".to_string()],
            allow_writes: false,
            allowed_tool_names: HashSet::from(["search".to_string()]),
            blocked_tool_names: HashSet::new(),
        };
        assert!(restore_external_http_binding(binding, Duration::from_secs(60)).is_err());
    }

    #[tokio::test]
    #[ignore = "requires CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL"]
    async fn mongodb_store_is_shared_across_service_instances() {
        let database_url = std::env::var("CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL")
            .expect("CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL");
        let session_id = format!("shared-store-test-{}", uuid::Uuid::new_v4());
        let first = RuntimeSessionStore::connect(
            database_url.as_str(),
            "shared-session-encryption-secret",
            Duration::from_secs(60),
        )
        .await
        .unwrap();
        let second = RuntimeSessionStore::connect(
            database_url.as_str(),
            "shared-session-encryption-secret",
            Duration::from_secs(60),
        )
        .await
        .unwrap();

        first.insert(snapshot(session_id.as_str())).await.unwrap();
        assert_eq!(
            second
                .get(session_id.as_str())
                .await
                .unwrap()
                .unwrap()
                .route_revision,
            "route-1"
        );
        assert!(second.remove(session_id.as_str()).await.unwrap().is_some());
        assert!(first.get(session_id.as_str()).await.unwrap().is_none());
    }
}
