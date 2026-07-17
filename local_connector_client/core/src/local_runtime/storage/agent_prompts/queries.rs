// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_plugin_management_sdk::{AgentPromptVendor, SystemAgentKey};

use super::{LocalAgentPromptRecord, LocalAgentPromptSyncState};
use crate::local_runtime::storage::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn get_agent_prompt_sync_state(
        &self,
    ) -> Result<Option<LocalAgentPromptSyncState>> {
        sqlx::query_as::<_, LocalAgentPromptSyncState>(
            r#"
            SELECT source_instance_id, installed_bundle_version, remote_bundle_version,
                   update_available, required, prompt_count, last_checked_at,
                   last_synced_at, last_error
            FROM system_agent_prompt_sync
            WHERE id = 1
            "#,
        )
        .fetch_optional(self.pool())
        .await
        .context("get local Agent Prompt sync state")
    }

    pub(crate) async fn get_installed_agent_prompt(
        &self,
        source_instance_id: &str,
        agent_key: SystemAgentKey,
        vendor: AgentPromptVendor,
    ) -> Result<Option<LocalAgentPromptRecord>> {
        sqlx::query_as::<_, LocalAgentPromptRecord>(
            r#"
            SELECT agent_key, vendor, content, revision, checksum, bundle_version,
                   published_at, synced_at, source_instance_id
            FROM system_agent_prompts
            WHERE source_instance_id = ? AND agent_key = ? AND vendor = ?
            "#,
        )
        .bind(source_instance_id)
        .bind(agent_key.as_str())
        .bind(vendor.as_str())
        .fetch_optional(self.pool())
        .await
        .context("get installed local Agent Prompt")
    }
}
