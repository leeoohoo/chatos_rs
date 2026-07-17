// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalAgentPromptRecord {
    pub(crate) agent_key: String,
    pub(crate) vendor: String,
    pub(crate) content: String,
    pub(crate) revision: i64,
    pub(crate) checksum: String,
    pub(crate) bundle_version: i64,
    pub(crate) published_at: String,
    pub(crate) synced_at: String,
    pub(crate) source_instance_id: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalAgentPromptSyncState {
    pub(crate) source_instance_id: String,
    pub(crate) installed_bundle_version: i64,
    pub(crate) remote_bundle_version: i64,
    pub(crate) update_available: bool,
    pub(crate) required: bool,
    pub(crate) prompt_count: i64,
    pub(crate) last_checked_at: Option<String>,
    pub(crate) last_synced_at: Option<String>,
    pub(crate) last_error: Option<String>,
}
