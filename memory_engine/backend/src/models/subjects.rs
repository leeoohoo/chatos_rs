// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_active;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSubject {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub subject_type: String,
    pub display_name: Option<String>,
    pub attributes: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSubjectRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_type: String,
    pub display_name: Option<String>,
    pub attributes: Option<Value>,
    pub status: Option<String>,
}
