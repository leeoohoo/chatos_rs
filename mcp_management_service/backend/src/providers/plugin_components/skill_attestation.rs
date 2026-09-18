// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use chatos_plugin_management_sdk::{
    SkillActivationAttestationClaims, DEFAULT_SKILL_ACTIVATION_LIMIT,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use super::THIRD_PARTY_PLUGIN_ENVELOPE;

const ACTIVATION_NONCE_BYTES: usize = 12;
const ACTIVATION_REFERENCE_PREFIX: &str = "SA";
const ACTIVATION_REFERENCE_RANDOM_HEX_LENGTH: usize = 32;
const MAX_PERSISTED_ACTIVATION_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ActiveSkillActivation {
    pub(crate) claims: SkillActivationAttestationClaims,
    pub(crate) parent_activation_ref: Option<String>,
    pub(crate) depth: u32,
    pub(crate) instructions: String,
}

pub(crate) struct SkillActivationAttestationService {
    store: SkillActivationStore,
}

enum SkillActivationStore {
    Memory(RwLock<HashMap<String, HashMap<String, ActiveSkillActivation>>>),
    Postgres(PostgresSkillActivationStore),
}

struct PostgresSkillActivationStore {
    pool: chatos_postgres::PgPool,
    cipher: ActivationCipher,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct StoredSkillActivationDocument {
    activation_ref: String,
    runtime_session_id: String,
    equivalence_sha256: String,
    expires_at: DateTime<Utc>,
    expires_at_unix: i64,
    nonce: Vec<u8>,
    encrypted_activation: Vec<u8>,
}

#[derive(Clone)]
struct ActivationCipher {
    key: [u8; 32],
}

impl SkillActivationAttestationService {
    pub(crate) fn new(secret: &str) -> Result<Self, String> {
        validate_activation_secret(secret)?;
        Ok(Self {
            store: SkillActivationStore::Memory(RwLock::new(HashMap::new())),
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn connect(secret: &str, database_url: &str) -> Result<Self, String> {
        validate_activation_secret(secret)?;
        let pool = crate::postgres::connect(database_url).await?;
        Self::from_pool(secret, pool).await
    }

    pub(crate) async fn from_pool(
        secret: &str,
        pool: chatos_postgres::PgPool,
    ) -> Result<Self, String> {
        validate_activation_secret(secret)?;
        verify_shared_activation_key(&pool, secret).await?;
        Ok(Self {
            store: SkillActivationStore::Postgres(PostgresSkillActivationStore {
                pool,
                cipher: ActivationCipher::new(secret)?,
            }),
        })
    }

    pub(crate) async fn activation(
        &self,
        runtime_session_id: &str,
        activation_ref: &str,
    ) -> Result<Option<ActiveSkillActivation>, String> {
        let now = chrono::Utc::now().timestamp();
        match &self.store {
            SkillActivationStore::Memory(activations) => {
                let mut activations = activations.write().await;
                activations.retain(|_, session| {
                    session.retain(|_, activation| activation.claims.expires_at_unix > now);
                    !session.is_empty()
                });
                Ok(activations
                    .get(runtime_session_id)
                    .and_then(|session| session.get(activation_ref))
                    .cloned())
            }
            SkillActivationStore::Postgres(store) => sqlx::query_as::<
                _,
                StoredSkillActivationDocument,
            >(
                "SELECT activation_ref,runtime_session_id,equivalence_sha256,expires_at,expires_at_unix,nonce,encrypted_activation \
                 FROM mcp_management_skill_activations WHERE activation_ref=$1 AND runtime_session_id=$2 AND expires_at_unix>$3",
            )
                .bind(activation_ref)
                .bind(runtime_session_id)
                .bind(now)
                .fetch_optional(&store.pool)
                .await
                .map_err(|error| format!("load Plugin Skill activation failed: {error}"))?
                .map(|document| store.cipher.decrypt(document))
                .transpose(),
        }
    }

    pub(crate) async fn find_equivalent(
        &self,
        claims: &SkillActivationAttestationClaims,
        parent_activation_ref: Option<&str>,
    ) -> Result<Option<ActiveSkillActivation>, String> {
        let equivalence_sha256 = activation_equivalence_sha256(claims, parent_activation_ref);
        let now = chrono::Utc::now().timestamp();
        match &self.store {
            SkillActivationStore::Memory(activations) => Ok(activations
                .read()
                .await
                .get(claims.runtime_session_id.as_str())
                .and_then(|session| {
                    session.values().find(|activation| {
                        activation.claims.expires_at_unix > now
                            && activation_equivalence_sha256(
                                &activation.claims,
                                activation.parent_activation_ref.as_deref(),
                            ) == equivalence_sha256
                    })
                })
                .cloned()),
            SkillActivationStore::Postgres(store) => sqlx::query_as::<
                _,
                StoredSkillActivationDocument,
            >(
                "SELECT activation_ref,runtime_session_id,equivalence_sha256,expires_at,expires_at_unix,nonce,encrypted_activation \
                 FROM mcp_management_skill_activations WHERE runtime_session_id=$1 AND equivalence_sha256=$2 \
                 AND expires_at_unix>$3 ORDER BY activation_ref LIMIT 1",
            )
                .bind(&claims.runtime_session_id)
                .bind(equivalence_sha256)
                .bind(now)
                .fetch_optional(&store.pool)
                .await
                .map_err(|error| {
                    format!("find equivalent Plugin Skill activation failed: {error}")
                })?
                .map(|document| store.cipher.decrypt(document))
                .transpose(),
        }
    }

    pub(crate) async fn register(
        &self,
        claims: SkillActivationAttestationClaims,
        parent_activation_ref: Option<String>,
        depth: u32,
        instructions: String,
    ) -> Result<ActiveSkillActivation, String> {
        validate_activation_reference(claims.activation_ref.as_str())?;
        let activation = ActiveSkillActivation {
            claims: claims.clone(),
            parent_activation_ref,
            depth,
            instructions,
        };
        match &self.store {
            SkillActivationStore::Memory(activations) => {
                let mut activations = activations.write().await;
                let session = activations
                    .entry(claims.runtime_session_id.clone())
                    .or_default();
                if session.len() >= DEFAULT_SKILL_ACTIVATION_LIMIT as usize {
                    return Err(format!(
                        "Plugin Skill activation limit exceeded ({DEFAULT_SKILL_ACTIVATION_LIMIT})"
                    ));
                }
                session.insert(claims.activation_ref.clone(), activation.clone());
            }
            SkillActivationStore::Postgres(store) => {
                let mut tx = store
                    .pool
                    .begin()
                    .await
                    .map_err(|error| error.to_string())?;
                sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                    .bind(&claims.runtime_session_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        format!("lock Plugin Skill activation session failed: {error}")
                    })?;
                let count = sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM mcp_management_skill_activations \
                     WHERE runtime_session_id=$1 AND expires_at_unix>$2",
                )
                .bind(&claims.runtime_session_id)
                .bind(chrono::Utc::now().timestamp())
                .fetch_one(&mut *tx)
                .await
                .map_err(|error| format!("count Plugin Skill activations failed: {error}"))?;
                if count >= i64::from(DEFAULT_SKILL_ACTIVATION_LIMIT) {
                    return Err(format!(
                        "Plugin Skill activation limit exceeded ({DEFAULT_SKILL_ACTIVATION_LIMIT})"
                    ));
                }
                let document = store.cipher.encrypt(&activation)?;
                sqlx::query(
                    "INSERT INTO mcp_management_skill_activations \
                     (activation_ref,runtime_session_id,equivalence_sha256,expires_at,expires_at_unix,nonce,encrypted_activation) \
                     VALUES($1,$2,$3,$4,$5,$6,$7)",
                )
                    .bind(&document.activation_ref)
                    .bind(&document.runtime_session_id)
                    .bind(&document.equivalence_sha256)
                    .bind(document.expires_at)
                    .bind(document.expires_at_unix)
                    .bind(&document.nonce)
                    .bind(&document.encrypted_activation)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| format!("persist Plugin Skill activation failed: {error}"))?;
                tx.commit().await.map_err(|error| error.to_string())?;
            }
        }
        Ok(activation)
    }

    pub(crate) async fn active_activations(
        &self,
        runtime_session_id: &str,
    ) -> Result<Vec<ActiveSkillActivation>, String> {
        let now = chrono::Utc::now().timestamp();
        let mut activations = match &self.store {
            SkillActivationStore::Memory(store) => store
                .read()
                .await
                .get(runtime_session_id)
                .map(|session| session.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
            SkillActivationStore::Postgres(store) => {
                let documents = sqlx::query_as::<_, StoredSkillActivationDocument>(
                    "SELECT activation_ref,runtime_session_id,equivalence_sha256,expires_at,expires_at_unix,nonce,encrypted_activation \
                     FROM mcp_management_skill_activations WHERE runtime_session_id=$1 AND expires_at_unix>$2 \
                     ORDER BY activation_ref",
                )
                    .bind(runtime_session_id)
                    .bind(now)
                    .fetch_all(&store.pool)
                    .await
                    .map_err(|error| format!("list Plugin Skill activations failed: {error}"))?;
                documents
                    .into_iter()
                    .map(|document| store.cipher.decrypt(document))
                    .collect::<Result<Vec<_>, _>>()?
            }
        };
        activations.retain(|activation| {
            activation.claims.runtime_session_id == runtime_session_id
                && activation.claims.issuer == "mcp-management-service"
                && activation.claims.audience == "plugin-skill-runtime"
                && activation.claims.expires_at_unix > now
        });
        activations.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then(left.claims.issued_at_unix.cmp(&right.claims.issued_at_unix))
                .then(left.claims.activation_ref.cmp(&right.claims.activation_ref))
        });
        Ok(activations)
    }

    pub(crate) async fn active_for_skill_ref(
        &self,
        runtime_session_id: &str,
        skill_ref: &str,
    ) -> Result<Option<ActiveSkillActivation>, String> {
        Ok(self
            .active_activations(runtime_session_id)
            .await?
            .into_iter()
            .rev()
            .find(|activation| activation.claims.skill_ref == skill_ref))
    }

    pub(crate) async fn protected_instruction_items(
        &self,
        runtime_session_id: &str,
    ) -> Result<Vec<Value>, String> {
        let activations = self.active_activations(runtime_session_id).await?;
        Ok(activations
            .into_iter()
            .map(|activation| protected_instruction_item(&activation))
            .collect())
    }

    pub(crate) async fn remove_session(&self, runtime_session_id: &str) -> Result<(), String> {
        match &self.store {
            SkillActivationStore::Memory(activations) => {
                activations.write().await.remove(runtime_session_id);
                Ok(())
            }
            SkillActivationStore::Postgres(store) => sqlx::query(
                "DELETE FROM mcp_management_skill_activations WHERE runtime_session_id=$1",
            )
            .bind(runtime_session_id)
            .execute(&store.pool)
            .await
            .map(|_| ())
            .map_err(|error| format!("remove Plugin Skill activations failed: {error}")),
        }
    }
}

fn validate_activation_secret(secret: &str) -> Result<(), String> {
    if secret.trim().len() < 16 {
        return Err("Plugin Skill attestation secret must contain at least 16 bytes".to_string());
    }
    Ok(())
}

async fn verify_shared_activation_key(
    pool: &chatos_postgres::PgPool,
    secret: &str,
) -> Result<(), String> {
    let fingerprint = hex::encode(Sha256::digest(
        format!("chatos.plugin.skill.activation.key.v1\0{}", secret.trim()).as_bytes(),
    ));
    sqlx::query(
        "INSERT INTO mcp_management_skill_activation_metadata(key,fingerprint_sha256,created_at) \
         VALUES('encryption-key-v1',$1,now()) ON CONFLICT(key) DO NOTHING",
    )
    .bind(&fingerprint)
    .execute(pool)
    .await
    .map_err(|error| format!("initialize Plugin Skill activation key metadata failed: {error}"))?;
    let stored = sqlx::query_scalar::<_, String>(
        "SELECT fingerprint_sha256 FROM mcp_management_skill_activation_metadata WHERE key='encryption-key-v1'",
    )
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("read Plugin Skill activation key metadata failed: {error}"))?
        .ok_or_else(|| "Plugin Skill activation key metadata is missing".to_string())?;
    if stored != fingerprint {
        return Err(
            "Plugin Skill activation encryption key does not match the key already registered by another MCP Management instance"
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn new_activation_reference() -> String {
    format!(
        "{ACTIVATION_REFERENCE_PREFIX}{}",
        uuid::Uuid::new_v4().simple()
    )
}

fn validate_activation_reference(value: &str) -> Result<(), String> {
    let suffix = value
        .strip_prefix(ACTIVATION_REFERENCE_PREFIX)
        .ok_or_else(|| "Plugin Skill activation evidence has an invalid reference".to_string())?;
    if suffix.len() != ACTIVATION_REFERENCE_RANDOM_HEX_LENGTH
        || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Plugin Skill activation evidence has an invalid reference".to_string());
    }
    Ok(())
}

fn activation_equivalence_sha256(
    claims: &SkillActivationAttestationClaims,
    parent_activation_ref: Option<&str>,
) -> String {
    let identity = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        claims.plugin_id,
        claims.release_id,
        claims.component_key,
        claims.arguments_sha256,
        claims.instructions_sha256,
        claims.resource_manifest_sha256,
        parent_activation_ref.unwrap_or_default(),
    );
    hex::encode(Sha256::digest(identity.as_bytes()))
}

fn protected_instruction_item(activation: &ActiveSkillActivation) -> Value {
    json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": format!(
                "[Protected Plugin Skill Context]\n{}\n\n<skill_activation name=\"{}\" skill_ref=\"{}\" depth=\"{}\" />\nThe platform tracks this activation internally. Do not add authentication, user, project, workspace, session, or activation identifiers to Plugin tool arguments.\n\n<skill_content name=\"{}\" depth=\"{}\">\n{}\n</skill_content>",
                THIRD_PARTY_PLUGIN_ENVELOPE,
                activation.claims.skill_name,
                activation.claims.skill_ref,
                activation.depth,
                activation.claims.skill_name,
                activation.depth,
                activation.instructions,
            )
        }],
        "_meta": {
            "chatos/pluginId": activation.claims.plugin_id,
            "chatos/releaseId": activation.claims.release_id,
            "chatos/instructionsSha256": activation.claims.instructions_sha256,
        }
    })
}

impl ActivationCipher {
    fn new(secret: &str) -> Result<Self, String> {
        let secret = secret.trim();
        if secret.is_empty() {
            return Err("Plugin Skill activation encryption secret cannot be empty".to_string());
        }
        let digest = Sha256::digest(secret.as_bytes());
        let mut key = [0_u8; 32];
        key.copy_from_slice(digest.as_slice());
        Ok(Self { key })
    }

    fn encrypt(
        &self,
        activation: &ActiveSkillActivation,
    ) -> Result<StoredSkillActivationDocument, String> {
        let plain = serde_json::to_vec(activation)
            .map_err(|error| format!("serialize Plugin Skill activation failed: {error}"))?;
        if plain.len() > MAX_PERSISTED_ACTIVATION_BYTES {
            return Err(format!(
                "Plugin Skill activation exceeds persisted size limit: {} bytes > {} bytes",
                plain.len(),
                MAX_PERSISTED_ACTIVATION_BYTES
            ));
        }
        let mut nonce = [0_u8; ACTIVATION_NONCE_BYTES];
        rand::fill(&mut nonce);
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|error| {
            format!("initialize Plugin Skill activation cipher failed: {error}")
        })?;
        let nonce_ref = Nonce::try_from(nonce.as_slice())
            .map_err(|error| format!("initialize Plugin Skill activation nonce failed: {error}"))?;
        let encrypted_activation = cipher
            .encrypt(
                &nonce_ref,
                Payload {
                    msg: plain.as_slice(),
                    aad: activation.claims.activation_ref.as_bytes(),
                },
            )
            .map_err(|error| format!("encrypt Plugin Skill activation failed: {error}"))?;
        Ok(StoredSkillActivationDocument {
            activation_ref: activation.claims.activation_ref.clone(),
            runtime_session_id: activation.claims.runtime_session_id.clone(),
            equivalence_sha256: activation_equivalence_sha256(
                &activation.claims,
                activation.parent_activation_ref.as_deref(),
            ),
            expires_at: DateTime::<Utc>::from_timestamp(activation.claims.expires_at_unix, 0)
                .ok_or_else(|| {
                    "Plugin Skill activation expiry is outside timestamp range".to_string()
                })?,
            expires_at_unix: activation.claims.expires_at_unix,
            nonce: nonce.to_vec(),
            encrypted_activation,
        })
    }

    fn decrypt(
        &self,
        document: StoredSkillActivationDocument,
    ) -> Result<ActiveSkillActivation, String> {
        if document.nonce.len() != ACTIVATION_NONCE_BYTES {
            return Err("Plugin Skill activation nonce has an invalid size".to_string());
        }
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|error| {
            format!("initialize Plugin Skill activation cipher failed: {error}")
        })?;
        let nonce_ref = Nonce::try_from(document.nonce.as_slice())
            .map_err(|error| format!("initialize Plugin Skill activation nonce failed: {error}"))?;
        let plain = cipher
            .decrypt(
                &nonce_ref,
                Payload {
                    msg: document.encrypted_activation.as_slice(),
                    aad: document.activation_ref.as_bytes(),
                },
            )
            .map_err(|_| {
                "decrypt Plugin Skill activation failed: key mismatch or corrupted data".to_string()
            })?;
        let activation = serde_json::from_slice::<ActiveSkillActivation>(&plain)
            .map_err(|error| format!("decode Plugin Skill activation failed: {error}"))?;
        if activation.claims.activation_ref != document.activation_ref
            || activation.claims.runtime_session_id != document.runtime_session_id
            || activation.claims.expires_at_unix != document.expires_at_unix
            || activation_equivalence_sha256(
                &activation.claims,
                activation.parent_activation_ref.as_deref(),
            ) != document.equivalence_sha256
        {
            return Err("Plugin Skill activation metadata does not match its envelope".to_string());
        }
        Ok(activation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(activation_ref: &str) -> SkillActivationAttestationClaims {
        SkillActivationAttestationClaims {
            issuer: "mcp-management-service".to_string(),
            audience: "plugin-skill-runtime".to_string(),
            tenant_id: "tenant-a".to_string(),
            owner_user_id: "user-a".to_string(),
            task_id: Some("task-a".to_string()),
            run_id: Some("run-a".to_string()),
            runtime_session_id: "session-a".to_string(),
            scope_kind: "project".to_string(),
            scope_id: "scope-a".to_string(),
            device_id: Some("device-a".to_string()),
            workspace_id: Some("workspace-a".to_string()),
            plugin_id: "plugin-a".to_string(),
            release_id: "release-a".to_string(),
            component_key: "router".to_string(),
            skill_ref: "SKrouter".to_string(),
            skill_name: "router".to_string(),
            activation_ref: activation_ref.to_string(),
            instructions_sha256: "a".repeat(64),
            resource_manifest_sha256: "b".repeat(64),
            arguments_sha256: "c".repeat(64),
            nonce: "nonce".to_string(),
            issued_at_unix: chrono::Utc::now().timestamp(),
            expires_at_unix: chrono::Utc::now().timestamp() + 3600,
        }
    }

    #[tokio::test]
    async fn memory_store_persists_verifies_and_composes_protected_context() {
        let service = SkillActivationAttestationService::new("0123456789abcdef").unwrap();
        let activation_ref = "SA0123456789abcdef0123456789abcdef";
        let activation = service
            .register(
                claims(activation_ref),
                None,
                0,
                "Follow the router rules.".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            service
                .active_for_skill_ref("session-a", activation.claims.skill_ref.as_str())
                .await
                .unwrap()
                .unwrap(),
            activation,
        );
        let items = service
            .protected_instruction_items("session-a")
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].to_string().contains("Follow the router rules."));
        let protected_text = items[0]
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap();
        assert!(!protected_text.contains(activation_ref));
        assert!(!protected_text.contains("activation_evidence"));
        service.remove_session("session-a").await.unwrap();
        assert!(service
            .activation("session-a", activation_ref)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn activation_state_is_internal_and_scoped_to_the_runtime_session() {
        let service = SkillActivationAttestationService::new("0123456789abcdef").unwrap();
        let activation_ref = "SAfedcba9876543210fedcba9876543210";
        let activation = service
            .register(
                claims(activation_ref),
                None,
                0,
                "Follow the router rules.".to_string(),
            )
            .await
            .unwrap();

        assert!(service
            .active_for_skill_ref("another-session", activation.claims.skill_ref.as_str())
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            service
                .active_for_skill_ref("session-a", activation.claims.skill_ref.as_str())
                .await
                .unwrap()
                .unwrap(),
            activation,
        );
    }

    #[test]
    fn encrypted_activation_roundtrip_binds_envelope_fields() {
        let cipher = ActivationCipher::new("0123456789abcdef").unwrap();
        let activation = ActiveSkillActivation {
            claims: claims("SA11111111111111111111111111111111"),
            parent_activation_ref: Some("SA00000000000000000000000000000000".to_string()),
            depth: 1,
            instructions: "Specialist rules".to_string(),
        };
        let document = cipher.encrypt(&activation).unwrap();
        assert_eq!(cipher.decrypt(document).unwrap(), activation);
    }

    #[tokio::test]
    #[ignore = "requires MCP_MANAGEMENT_TEST_DATABASE_URL and migrated PostgreSQL"]
    async fn postgresql_store_shares_internal_activation_state_and_rejects_key_drift() {
        let database_url = std::env::var("MCP_MANAGEMENT_TEST_DATABASE_URL").unwrap();
        let secret = "shared-skill-activation-secret";
        let first = SkillActivationAttestationService::connect(secret, database_url.as_str())
            .await
            .unwrap();
        let second = SkillActivationAttestationService::connect(secret, database_url.as_str())
            .await
            .unwrap();
        let session_id = format!("skill-session-{}", uuid::Uuid::new_v4().simple());
        let activation_ref = new_activation_reference();
        let mut shared_claims = claims(activation_ref.as_str());
        shared_claims.runtime_session_id = session_id.clone();
        let activation = first
            .register(
                shared_claims,
                None,
                0,
                "Shared specialist rules".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(
            second
                .active_for_skill_ref(session_id.as_str(), activation.claims.skill_ref.as_str())
                .await
                .unwrap()
                .unwrap(),
            activation,
        );
        assert!(SkillActivationAttestationService::connect(
            "different-skill-activation-secret",
            database_url.as_str(),
        )
        .await
        .is_err());
        first.remove_session(session_id.as_str()).await.unwrap();
    }
}
