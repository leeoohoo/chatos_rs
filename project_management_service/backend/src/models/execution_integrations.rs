// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectExecutionIntegrationStatus {
    Active,
    Blocked,
    ReadyToPromote,
    Promoting,
    Promoted,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectExecutionIntegrationRecord {
    pub id: String,
    pub project_id: String,
    pub execution_group_id: String,
    pub target_branch: String,
    pub execution_branch_ref: String,
    pub initial_base_commit: String,
    pub current_head_commit: String,
    pub status: ProjectExecutionIntegrationStatus,
    #[serde(default)]
    pub lock_owner_worker_id: Option<String>,
    #[serde(default)]
    pub lease_token: Option<String>,
    #[serde(default)]
    pub lease_until: Option<String>,
    #[serde(default)]
    pub lock_version: i64,
    #[serde(default)]
    pub promoted_commit: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBranchPromotionLeaseRecord {
    pub id: String,
    pub project_id: String,
    pub target_branch: String,
    #[serde(default)]
    pub owner_worker_id: Option<String>,
    #[serde(default)]
    pub lease_token: Option<String>,
    #[serde(default)]
    pub lease_until: Option<String>,
    #[serde(default)]
    pub lock_version: i64,
    pub created_at: String,
    pub updated_at: String,
}
