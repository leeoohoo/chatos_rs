// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub use memory_engine_sdk::{
    EngineSubjectMemoryScope, RunSubjectMemoryScopesResponse, UpsertSubjectMemoryScopeRequest,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSubjectMemoryScopesRequest {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub limit: Option<i64>,
}
