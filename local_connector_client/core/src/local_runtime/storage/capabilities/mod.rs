// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_plugin_management_sdk::ResolvedAgentCapabilities;

use crate::local_now_rfc3339;

use super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn save_capability_snapshot(
        &self,
        capabilities: &ResolvedAgentCapabilities,
    ) -> Result<()> {
        validate_snapshot(capabilities)?;
        let synced_at = local_now_rfc3339();
        let generated_at = normalized_generated_at(capabilities, synced_at.as_str());
        let payload_json =
            serde_json::to_string(capabilities).context("encode local capability snapshot")?;
        sqlx::query(
            r#"
            INSERT INTO agent_capability_snapshots (
                owner_user_id, agent_key, policy_revision, payload_json,
                generated_at, synced_at
            ) VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(owner_user_id, agent_key) DO UPDATE SET
                policy_revision = excluded.policy_revision,
                payload_json = excluded.payload_json,
                generated_at = excluded.generated_at,
                synced_at = excluded.synced_at
            "#,
        )
        .bind(capabilities.owner_user_id.as_str())
        .bind(capabilities.agent_key.as_str())
        .bind(capabilities.policy_revision.as_str())
        .bind(payload_json)
        .bind(generated_at)
        .bind(synced_at)
        .execute(self.pool())
        .await
        .context("save local capability snapshot")?;
        Ok(())
    }

    pub(crate) async fn get_capability_snapshot(
        &self,
        owner_user_id: &str,
        agent_key: &str,
    ) -> Result<Option<ResolvedAgentCapabilities>> {
        let payload = sqlx::query_scalar::<_, String>(
            "SELECT payload_json FROM agent_capability_snapshots WHERE owner_user_id = ? AND agent_key = ?",
        )
        .bind(owner_user_id)
        .bind(agent_key)
        .fetch_optional(self.pool())
        .await
        .context("get local capability snapshot")?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let snapshot = serde_json::from_str::<ResolvedAgentCapabilities>(payload.as_str())
            .context("decode local capability snapshot")?;
        validate_snapshot_identity(&snapshot, owner_user_id, agent_key)?;
        Ok(Some(snapshot))
    }
}

fn validate_snapshot(capabilities: &ResolvedAgentCapabilities) -> Result<()> {
    if capabilities.owner_user_id.trim().is_empty() || capabilities.agent_key.trim().is_empty() {
        return Err(anyhow::anyhow!("capability snapshot identity is required"));
    }
    Ok(())
}

fn validate_snapshot_identity(
    capabilities: &ResolvedAgentCapabilities,
    owner_user_id: &str,
    agent_key: &str,
) -> Result<()> {
    if capabilities.owner_user_id != owner_user_id || capabilities.agent_key != agent_key {
        return Err(anyhow::anyhow!(
            "capability snapshot identity does not match its SQLite key"
        ));
    }
    Ok(())
}

fn normalized_generated_at(capabilities: &ResolvedAgentCapabilities, fallback: &str) -> String {
    let value = capabilities.generated_at.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests;
