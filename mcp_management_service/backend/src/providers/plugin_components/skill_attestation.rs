// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use chatos_plugin_management_sdk::{
    SkillActivationAttestationClaims, DEFAULT_SKILL_ACTIVATION_LIMIT,
};
use futures_util::TryStreamExt;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use mongodb::bson::{doc, spec::BinarySubtype, Binary, DateTime};
use mongodb::options::IndexOptions;
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use super::THIRD_PARTY_PLUGIN_ENVELOPE;

const ACTIVATION_NONCE_BYTES: usize = 12;
const MAX_PERSISTED_ACTIVATION_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ActiveSkillActivation {
    pub(crate) claims: SkillActivationAttestationClaims,
    pub(crate) parent_activation_ref: Option<String>,
    pub(crate) depth: u32,
    pub(crate) evidence: String,
    pub(crate) instructions: String,
}

pub(crate) struct SkillActivationAttestationService {
    encoding: EncodingKey,
    decoding: DecodingKey,
    store: SkillActivationStore,
}

enum SkillActivationStore {
    Memory(RwLock<HashMap<String, HashMap<String, ActiveSkillActivation>>>),
    Mongo(MongoSkillActivationStore),
}

struct MongoSkillActivationStore {
    collection: Collection<StoredSkillActivationDocument>,
    cipher: ActivationCipher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSkillActivationDocument {
    #[serde(rename = "_id")]
    activation_ref: String,
    runtime_session_id: String,
    equivalence_sha256: String,
    expires_at: DateTime,
    expires_at_unix: i64,
    nonce: Binary,
    encrypted_activation: Binary,
}

#[derive(Clone)]
struct ActivationCipher {
    key: [u8; 32],
}

impl SkillActivationAttestationService {
    pub(crate) fn new(secret: &str) -> Result<Self, String> {
        let (encoding, decoding) = signing_keys(secret)?;
        Ok(Self {
            encoding,
            decoding,
            store: SkillActivationStore::Memory(RwLock::new(HashMap::new())),
        })
    }

    pub(crate) async fn connect(secret: &str, database_url: &str) -> Result<Self, String> {
        let (encoding, decoding) = signing_keys(secret)?;
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|error| format!("connect Plugin Skill activation MongoDB failed: {error}"))?;
        let database = client.default_database().ok_or_else(|| {
            "MCP_MANAGEMENT_DATABASE_URL must include a MongoDB database name".to_string()
        })?;
        let collection = database
            .collection::<StoredSkillActivationDocument>("mcp_management_skill_activations");
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "expires_at": 1 })
                    .options(
                        IndexOptions::builder()
                            .name("skill_activation_expiry_ttl".to_string())
                            .expire_after(Some(Duration::from_secs(0)))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| {
                format!("initialize Plugin Skill activation TTL index failed: {error}")
            })?;
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! {
                        "runtime_session_id": 1,
                        "equivalence_sha256": 1,
                        "expires_at_unix": 1,
                    })
                    .options(
                        IndexOptions::builder()
                            .name("skill_activation_session_equivalence".to_string())
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| {
                format!("initialize Plugin Skill activation lookup index failed: {error}")
            })?;
        Ok(Self {
            encoding,
            decoding,
            store: SkillActivationStore::Mongo(MongoSkillActivationStore {
                collection,
                cipher: ActivationCipher::new(secret)?,
            }),
        })
    }

    pub(crate) fn issue(
        &self,
        claims: &SkillActivationAttestationClaims,
    ) -> Result<String, String> {
        jsonwebtoken::encode(&Header::new(Algorithm::HS256), claims, &self.encoding)
            .map_err(|error| format!("issue Plugin Skill activation evidence failed: {error}"))
    }

    pub(crate) fn verify(&self, token: &str) -> Result<SkillActivationAttestationClaims, String> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        validation.required_spec_claims.clear();
        let claims = jsonwebtoken::decode::<SkillActivationAttestationClaims>(
            token,
            &self.decoding,
            &validation,
        )
        .map_err(|error| format!("Plugin Skill activation evidence is invalid: {error}"))?
        .claims;
        if claims.issuer != "mcp-management-service"
            || claims.audience != "plugin-skill-runtime"
            || claims.expires_at_unix <= chrono::Utc::now().timestamp()
        {
            return Err(
                "Plugin Skill activation evidence has expired or has an invalid audience"
                    .to_string(),
            );
        }
        Ok(claims)
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
            SkillActivationStore::Mongo(store) => store
                .collection
                .find_one(
                    doc! {
                        "_id": activation_ref,
                        "runtime_session_id": runtime_session_id,
                        "expires_at_unix": { "$gt": now },
                    },
                    None,
                )
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
            SkillActivationStore::Mongo(store) => store
                .collection
                .find_one(
                    doc! {
                        "runtime_session_id": claims.runtime_session_id.as_str(),
                        "equivalence_sha256": equivalence_sha256,
                        "expires_at_unix": { "$gt": now },
                    },
                    None,
                )
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
        let evidence = self.issue(&claims)?;
        let activation = ActiveSkillActivation {
            claims: claims.clone(),
            parent_activation_ref,
            depth,
            evidence,
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
            SkillActivationStore::Mongo(store) => {
                let count = store
                    .collection
                    .count_documents(
                        doc! {
                            "runtime_session_id": claims.runtime_session_id.as_str(),
                            "expires_at_unix": { "$gt": chrono::Utc::now().timestamp() },
                        },
                        None,
                    )
                    .await
                    .map_err(|error| format!("count Plugin Skill activations failed: {error}"))?;
                if count >= u64::from(DEFAULT_SKILL_ACTIVATION_LIMIT) {
                    return Err(format!(
                        "Plugin Skill activation limit exceeded ({DEFAULT_SKILL_ACTIVATION_LIMIT})"
                    ));
                }
                let document = store.cipher.encrypt(&activation)?;
                store
                    .collection
                    .insert_one(document, None)
                    .await
                    .map_err(|error| format!("persist Plugin Skill activation failed: {error}"))?;
            }
        }
        Ok(activation)
    }

    pub(crate) async fn verify_active(&self, token: &str) -> Result<ActiveSkillActivation, String> {
        let claims = self.verify(token)?;
        let activation = self
            .activation(
                claims.runtime_session_id.as_str(),
                claims.activation_ref.as_str(),
            )
            .await?
            .ok_or_else(|| {
                "Plugin Skill activation evidence is not active in this Runtime Session".to_string()
            })?;
        if activation.claims != claims || activation.evidence != token {
            return Err(
                "Plugin Skill activation evidence does not match the active invocation graph"
                    .to_string(),
            );
        }
        Ok(activation)
    }

    pub(crate) async fn protected_instruction_items(
        &self,
        runtime_session_id: &str,
    ) -> Result<Vec<Value>, String> {
        let now = chrono::Utc::now().timestamp();
        let mut activations = match &self.store {
            SkillActivationStore::Memory(store) => store
                .read()
                .await
                .get(runtime_session_id)
                .map(|session| session.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
            SkillActivationStore::Mongo(store) => {
                let documents = store
                    .collection
                    .find(
                        doc! {
                            "runtime_session_id": runtime_session_id,
                            "expires_at_unix": { "$gt": now },
                        },
                        None,
                    )
                    .await
                    .map_err(|error| format!("list Plugin Skill activations failed: {error}"))?
                    .try_collect::<Vec<_>>()
                    .await
                    .map_err(|error| format!("read Plugin Skill activations failed: {error}"))?;
                documents
                    .into_iter()
                    .map(|document| store.cipher.decrypt(document))
                    .collect::<Result<Vec<_>, _>>()?
            }
        };
        activations.retain(|activation| activation.claims.expires_at_unix > now);
        activations.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then(left.claims.issued_at_unix.cmp(&right.claims.issued_at_unix))
                .then(left.claims.activation_ref.cmp(&right.claims.activation_ref))
        });
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
            SkillActivationStore::Mongo(store) => store
                .collection
                .delete_many(doc! { "runtime_session_id": runtime_session_id }, None)
                .await
                .map(|_| ())
                .map_err(|error| format!("remove Plugin Skill activations failed: {error}")),
        }
    }
}

fn signing_keys(secret: &str) -> Result<(EncodingKey, DecodingKey), String> {
    let secret = secret.trim().as_bytes();
    if secret.len() < 16 {
        return Err("Plugin Skill attestation secret must contain at least 16 bytes".to_string());
    }
    Ok((
        EncodingKey::from_secret(secret),
        DecodingKey::from_secret(secret),
    ))
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
                "[Protected Plugin Skill Context]\n{}\n\n<skill_content name=\"{}\" activation_ref=\"{}\" depth=\"{}\">\n{}\n</skill_content>",
                THIRD_PARTY_PLUGIN_ENVELOPE,
                activation.claims.skill_name,
                activation.claims.activation_ref,
                activation.depth,
                activation.instructions,
            )
        }],
        "_meta": {
            "chatos/protectedSkillActivationRef": activation.claims.activation_ref,
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
            expires_at: DateTime::from_millis(
                activation.claims.expires_at_unix.saturating_mul(1_000),
            ),
            expires_at_unix: activation.claims.expires_at_unix,
            nonce: Binary {
                subtype: BinarySubtype::Generic,
                bytes: nonce.to_vec(),
            },
            encrypted_activation: Binary {
                subtype: BinarySubtype::Generic,
                bytes: encrypted_activation,
            },
        })
    }

    fn decrypt(
        &self,
        document: StoredSkillActivationDocument,
    ) -> Result<ActiveSkillActivation, String> {
        if document.nonce.bytes.len() != ACTIVATION_NONCE_BYTES {
            return Err("Plugin Skill activation nonce has an invalid size".to_string());
        }
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|error| {
            format!("initialize Plugin Skill activation cipher failed: {error}")
        })?;
        let nonce_ref = Nonce::try_from(document.nonce.bytes.as_slice())
            .map_err(|error| format!("initialize Plugin Skill activation nonce failed: {error}"))?;
        let plain = cipher
            .decrypt(
                &nonce_ref,
                Payload {
                    msg: document.encrypted_activation.bytes.as_slice(),
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
        let activation = service
            .register(
                claims("SA1"),
                None,
                0,
                "Follow the router rules.".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            service
                .verify_active(activation.evidence.as_str())
                .await
                .unwrap(),
            activation
        );
        let items = service
            .protected_instruction_items("session-a")
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].to_string().contains("Follow the router rules."));
        service.remove_session("session-a").await.unwrap();
        assert!(service
            .activation("session-a", "SA1")
            .await
            .unwrap()
            .is_none());
    }

    #[test]
    fn encrypted_activation_roundtrip_binds_envelope_fields() {
        let cipher = ActivationCipher::new("0123456789abcdef").unwrap();
        let activation = ActiveSkillActivation {
            claims: claims("SA2"),
            parent_activation_ref: Some("SA1".to_string()),
            depth: 1,
            evidence: "signed-evidence".to_string(),
            instructions: "Specialist rules".to_string(),
        };
        let document = cipher.encrypt(&activation).unwrap();
        assert_eq!(cipher.decrypt(document).unwrap(), activation);
    }
}
