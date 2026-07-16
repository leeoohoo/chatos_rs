// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_active;
pub use memory_engine_sdk::{EngineSource, RotateSourceSecretResponse, UpsertSourceRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEngineSource {
    pub id: String,
    pub tenant_id: Option<String>,
    pub source_id: String,
    pub source_type: String,
    pub name: String,
    pub description: Option<String>,
    pub config: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default)]
    pub sdk_enabled: bool,
    pub secret_key_hint: Option<String>,
    pub key_last_rotated_at: Option<String>,
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    pub secret_key_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRotateSourceSecretResponse {
    pub source: StoredEngineSource,
    pub secret_key: String,
}

impl From<StoredEngineSource> for EngineSource {
    fn from(source: StoredEngineSource) -> Self {
        Self {
            id: source.id,
            tenant_id: source.tenant_id,
            source_id: source.source_id,
            source_type: source.source_type,
            name: source.name,
            description: source.description,
            config: source.config,
            status: source.status,
            sdk_enabled: source.sdk_enabled,
            secret_key_hint: source.secret_key_hint,
            key_last_rotated_at: source.key_last_rotated_at,
            created_at: source.created_at,
            updated_at: source.updated_at,
        }
    }
}

impl From<EngineSource> for StoredEngineSource {
    fn from(source: EngineSource) -> Self {
        Self {
            id: source.id,
            tenant_id: source.tenant_id,
            source_id: source.source_id,
            source_type: source.source_type,
            name: source.name,
            description: source.description,
            config: source.config,
            status: source.status,
            sdk_enabled: source.sdk_enabled,
            secret_key_hint: source.secret_key_hint,
            key_last_rotated_at: source.key_last_rotated_at,
            secret_key_hash: None,
            created_at: source.created_at,
            updated_at: source.updated_at,
        }
    }
}

impl From<StoredRotateSourceSecretResponse> for RotateSourceSecretResponse {
    fn from(response: StoredRotateSourceSecretResponse) -> Self {
        Self {
            source: response.source.into(),
            secret_key: response.secret_key,
        }
    }
}

impl From<RotateSourceSecretResponse> for StoredRotateSourceSecretResponse {
    fn from(response: RotateSourceSecretResponse) -> Self {
        Self {
            source: response.source.into(),
            secret_key: response.secret_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EngineSource, StoredEngineSource};

    #[test]
    fn engine_source_deserializes_stored_secret_hash_but_skips_it_when_serializing() {
        let source: StoredEngineSource = serde_json::from_value(serde_json::json!({
            "id": "src-1",
            "tenant_id": "tenant-1",
            "source_id": "source-1",
            "source_type": "sdk",
            "name": "Test Source",
            "description": null,
            "config": null,
            "status": "active",
            "sdk_enabled": true,
            "secret_key_hint": "mk_***1234",
            "key_last_rotated_at": "2026-05-21T00:00:00Z",
            "secret_key_hash": "hashed-secret",
            "created_at": "2026-05-21T00:00:00Z",
            "updated_at": "2026-05-21T00:00:00Z"
        }))
        .expect("source");

        assert_eq!(source.secret_key_hash.as_deref(), Some("hashed-secret"));

        let serialized = serde_json::to_value(&source).expect("serialize source");
        assert!(serialized.get("secret_key_hash").is_none());
    }

    #[test]
    fn stored_source_round_trips_through_sdk_contract_without_exposing_secret_hash() {
        let stored = StoredEngineSource {
            id: "src-1".to_string(),
            tenant_id: Some("tenant-1".to_string()),
            source_id: "source-1".to_string(),
            source_type: "sdk".to_string(),
            name: "Source".to_string(),
            description: Some("demo".to_string()),
            config: Some(serde_json::json!({"mode": "safe"})),
            status: "active".to_string(),
            sdk_enabled: true,
            secret_key_hint: Some("...123456".to_string()),
            key_last_rotated_at: Some("2026-05-21T00:00:00Z".to_string()),
            secret_key_hash: Some("hashed-secret".to_string()),
            created_at: "2026-05-20T00:00:00Z".to_string(),
            updated_at: "2026-05-21T00:00:00Z".to_string(),
        };

        let contract: EngineSource = stored.clone().into();
        let snapshot = serde_json::to_value(&contract).expect("serialize SDK source");
        assert_eq!(
            snapshot,
            serde_json::json!({
                "id": "src-1",
                "tenant_id": "tenant-1",
                "source_id": "source-1",
                "source_type": "sdk",
                "name": "Source",
                "description": "demo",
                "config": {"mode": "safe"},
                "status": "active",
                "sdk_enabled": true,
                "secret_key_hint": "...123456",
                "key_last_rotated_at": "2026-05-21T00:00:00Z",
                "created_at": "2026-05-20T00:00:00Z",
                "updated_at": "2026-05-21T00:00:00Z"
            })
        );

        let restored: StoredEngineSource = contract.into();
        assert_eq!(restored.source_id, stored.source_id);
        assert_eq!(restored.config, stored.config);
        assert!(restored.secret_key_hash.is_none());
    }
}
