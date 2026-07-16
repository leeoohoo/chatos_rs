// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use memory_engine_sdk::{DeleteThreadResponse, EngineThread, GetThreadResponse};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertThreadRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub thread_type: String,
    pub external_thread_id: Option<String>,
    pub title: Option<String>,
    pub labels: Option<Vec<String>>,
    pub metadata: Option<Value>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListThreadsByLabelRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_label: String,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
