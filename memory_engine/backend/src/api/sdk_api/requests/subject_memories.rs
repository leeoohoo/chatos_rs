// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SdkUpsertSubjectMemoryScopeRequest {
    pub tenant_id: String,
    pub subject_id: String,
    pub memory_type: String,
    pub source_thread_label: String,
    pub relation_subject_id: Option<String>,
    pub source_summary_type: Option<String>,
    pub prompt_title: Option<String>,
    pub memory_metadata: Option<serde_json::Value>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkQuerySubjectMemoriesRequest {
    pub tenant_id: String,
    pub subject_id: String,
    pub memory_type: Option<String>,
    pub level: Option<i64>,
    pub max_level_exclusive: Option<i64>,
    pub rollup_status: Option<String>,
    pub relation_subject_id: Option<String>,
    pub source_digest: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SdkListSummariesByThreadLabelRequest {
    pub tenant_id: String,
    pub thread_label: String,
    pub summary_type: Option<String>,
    pub status: Option<String>,
    pub level: Option<i64>,
    pub subject_memory_summarized: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
