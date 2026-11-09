// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub use memory_engine_sdk::{
    DashboardOverviewResponse, EngineJobPolicy, EngineJobRun, EngineModelProfile,
    JobRunsBundleResponse,
};

#[derive(Debug, Clone)]
pub struct CreateEngineJobRunRequest {
    pub job_type: String,
    pub trigger_type: String,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject_id: Option<String>,
    pub thread_label: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct FinishEngineJobRunRequest {
    pub status: String,
    pub input_count: i64,
    pub output_count: i64,
    pub processed_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub metadata: Option<Value>,
    pub error_message: Option<String>,
}
