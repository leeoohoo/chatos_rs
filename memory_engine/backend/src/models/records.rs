// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub use memory_engine_sdk::{
    BatchSyncRecordsResponse, CompactTurnsResponse, EngineRecord, ThreadRecordsPageResponse,
    TurnProcessRecordsResponse, TurnRecordSlice, UpsertRecordInput,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSyncRecordsRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub records: Vec<UpsertRecordInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineCompactTurn {
    pub id: String,
    pub thread_id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub record_type: String,
    pub turn_id: String,
    pub user_record_id: String,
    pub user_created_at: String,
    pub user_record: EngineRecord,
    pub final_assistant_record: Option<EngineRecord>,
    pub has_process: bool,
    pub tool_call_count: usize,
    pub thinking_count: usize,
    pub process_message_count: usize,
    pub updated_at: String,
}
