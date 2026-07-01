// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ListThreadRecordsQuery {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteThreadRecordsQuery {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub record_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CountThreadRecordsQuery {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CompactTurnsQuery {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub record_type: Option<String>,
    pub limit: Option<i64>,
    pub before_turn_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TurnProcessRecordsQuery {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub record_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetThreadQuery {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteThreadQuery {
    pub tenant_id: String,
    pub source_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AdminListThreadsQuery {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub subject_id: Option<String>,
    pub external_thread_id: Option<String>,
    pub session_id: Option<String>,
    pub contact_id: Option<String>,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub mapping_source: Option<String>,
    pub mapping_version: Option<String>,
    pub thread_label: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
