use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_active;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSource {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub source_type: String,
    pub name: String,
    pub config: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSourceRequest {
    pub tenant_id: String,
    pub source_type: String,
    pub name: String,
    pub config: Option<Value>,
    pub status: Option<String>,
}
