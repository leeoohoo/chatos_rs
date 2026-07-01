// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct JobRunsQuery {
    pub job_type: Option<String>,
    pub trigger_type: Option<String>,
    pub thread_id: Option<String>,
    pub status: Option<String>,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct JobRunStatsQuery {
    pub job_type: Option<String>,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub since_hours: Option<i64>,
}
