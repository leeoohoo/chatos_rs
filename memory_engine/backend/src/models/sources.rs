use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_active;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSource {
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
pub struct UpsertSourceRequest {
    pub tenant_id: Option<String>,
    pub source_type: String,
    pub name: String,
    pub description: Option<String>,
    pub config: Option<Value>,
    pub sdk_enabled: Option<bool>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotateSourceSecretResponse {
    pub source: EngineSource,
    pub secret_key: String,
}

#[cfg(test)]
mod tests {
    use super::EngineSource;

    #[test]
    fn engine_source_deserializes_stored_secret_hash_but_skips_it_when_serializing() {
        let source: EngineSource = serde_json::from_value(serde_json::json!({
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
}
