// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, SystemAgentKey};
use std::collections::HashSet;

use crate::local_now_rfc3339;

use super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn replace_capability_snapshots(
        &self,
        snapshots: &[ResolvedAgentCapabilities],
    ) -> Result<()> {
        let owner_user_id = validate_complete_snapshot_set(snapshots)?;
        let synced_at = local_now_rfc3339();
        let mut transaction = self.begin_write().await?;
        sqlx::query("DELETE FROM agent_capability_snapshots WHERE owner_user_id = ?")
            .bind(owner_user_id)
            .execute(&mut *transaction)
            .await
            .context("clear local capability snapshots")?;
        for capabilities in snapshots {
            let generated_at = normalized_generated_at(capabilities, synced_at.as_str());
            let payload_json =
                serde_json::to_string(capabilities).context("encode local capability snapshot")?;
            sqlx::query(
                r#"
                INSERT INTO agent_capability_snapshots (
                    owner_user_id, agent_key, policy_revision, payload_json,
                    generated_at, synced_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(capabilities.owner_user_id.as_str())
            .bind(capabilities.agent_key.as_str())
            .bind(capabilities.policy_revision.as_str())
            .bind(payload_json)
            .bind(generated_at)
            .bind(synced_at.as_str())
            .execute(&mut *transaction)
            .await
            .context("insert local capability snapshot")?;
        }
        transaction
            .commit()
            .await
            .context("commit local capability snapshots")
    }

    pub(crate) async fn count_capability_snapshots(&self, owner_user_id: &str) -> Result<i64> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_capability_snapshots WHERE owner_user_id = ?",
        )
        .bind(owner_user_id)
        .fetch_one(self.pool())
        .await
        .context("count local capability snapshots")
    }

    pub(crate) async fn capability_snapshot_set_is_complete(
        &self,
        owner_user_id: &str,
    ) -> Result<bool> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT agent_key, policy_revision FROM agent_capability_snapshots WHERE owner_user_id = ?",
        )
        .bind(owner_user_id)
        .fetch_all(self.pool())
        .await
        .context("validate local capability snapshot set")?;
        if rows.len() != SystemAgentKey::ALL.len()
            || rows.iter().any(|(_, revision)| revision.trim().is_empty())
        {
            return Ok(false);
        }
        let actual = rows
            .iter()
            .map(|(agent_key, _)| agent_key.as_str())
            .collect::<HashSet<_>>();
        let expected = SystemAgentKey::ALL
            .into_iter()
            .map(|agent_key| agent_key.as_str())
            .collect::<HashSet<_>>();
        Ok(actual == expected)
    }

    pub(crate) async fn capability_snapshots_match(
        &self,
        owner_user_id: &str,
        snapshots: &[ResolvedAgentCapabilities],
    ) -> Result<bool> {
        if validate_complete_snapshot_set(snapshots)? != owner_user_id {
            return Ok(false);
        }
        for snapshot in snapshots {
            let stored = sqlx::query_as::<_, (String, String)>(
                "SELECT policy_revision, payload_json FROM agent_capability_snapshots WHERE owner_user_id = ? AND agent_key = ?",
            )
            .bind(owner_user_id)
            .bind(snapshot.agent_key.as_str())
            .fetch_optional(self.pool())
            .await
            .context("compare local capability snapshot")?;
            let Some((stored_revision, stored_payload)) = stored else {
                return Ok(false);
            };
            if stored_revision != snapshot.policy_revision {
                return Ok(false);
            }
            let mut stored_snapshot =
                serde_json::from_str::<ResolvedAgentCapabilities>(stored_payload.as_str())
                    .context("decode local capability snapshot for comparison")?;
            let mut remote_snapshot = snapshot.clone();
            stored_snapshot.generated_at.clear();
            remote_snapshot.generated_at.clear();
            if serde_json::to_value(stored_snapshot)
                .context("encode stored capability snapshot for comparison")?
                != serde_json::to_value(remote_snapshot)
                    .context("encode remote capability snapshot for comparison")?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    #[cfg(test)]
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

pub(crate) fn validate_complete_snapshot_set(
    snapshots: &[ResolvedAgentCapabilities],
) -> Result<&str> {
    if snapshots.len() != SystemAgentKey::ALL.len() {
        return Err(anyhow::anyhow!(
            "system Agent capability bundle is incomplete: expected {}, got {}",
            SystemAgentKey::ALL.len(),
            snapshots.len()
        ));
    }
    let owner_user_id = snapshots
        .first()
        .map(|snapshot| snapshot.owner_user_id.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("capability snapshot owner is required"))?;
    let expected = SystemAgentKey::ALL
        .into_iter()
        .map(|agent_key| agent_key.as_str())
        .collect::<HashSet<_>>();
    let mut actual = HashSet::new();
    for snapshot in snapshots {
        validate_snapshot(snapshot)?;
        if snapshot.owner_user_id != owner_user_id || !actual.insert(snapshot.agent_key.as_str()) {
            return Err(anyhow::anyhow!(
                "system Agent capability bundle identity is inconsistent"
            ));
        }
    }
    if actual != expected {
        return Err(anyhow::anyhow!(
            "system Agent capability bundle does not cover every configured Agent"
        ));
    }
    Ok(owner_user_id)
}

fn validate_snapshot(capabilities: &ResolvedAgentCapabilities) -> Result<()> {
    if capabilities.owner_user_id.trim().is_empty()
        || capabilities.agent_key.trim().is_empty()
        || capabilities.policy_revision.trim().is_empty()
    {
        return Err(anyhow::anyhow!("capability snapshot identity is required"));
    }
    if !SystemAgentKey::ALL
        .into_iter()
        .any(|agent_key| agent_key.as_str() == capabilities.agent_key.as_str())
    {
        return Err(anyhow::anyhow!(
            "capability snapshot contains an unknown system Agent: {}",
            capabilities.agent_key
        ));
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
