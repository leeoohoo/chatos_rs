// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SdkListThreadSummariesRequest {
    pub tenant_id: String,
    pub summary_type: Option<String>,
    pub status: Option<String>,
    pub level: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SdkDeleteThreadSummaryRequest {
    pub tenant_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SdkRunThreadSummaryRequest {
    pub tenant_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SdkRunThreadActiveSummaryRequest {
    pub tenant_id: String,
    pub trigger_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkGetThreadActiveSummaryStatusRequest {
    pub tenant_id: String,
    pub job_run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkRunThreadRepairSummaryRequest {
    pub tenant_id: String,
}
