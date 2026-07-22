// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_plugin_management_sdk::{
    AgentPromptBundle, AgentPromptBundleManifest, ResolvedAgentCapabilities,
};

use crate::local_now_rfc3339;
use crate::local_runtime::storage::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn save_agent_prompt_manifest(
        &self,
        source_instance_id: &str,
        manifest: &AgentPromptBundleManifest,
        capability_update_available: bool,
    ) -> Result<()> {
        let current = self.get_agent_prompt_sync_state().await?;
        let same_source = current
            .as_ref()
            .is_some_and(|state| state.source_instance_id == source_instance_id);
        let installed_version = if same_source {
            current
                .as_ref()
                .map(|state| state.installed_bundle_version)
                .unwrap_or_default()
        } else {
            0
        };
        let prompt_count = if same_source {
            current
                .as_ref()
                .map(|state| state.prompt_count)
                .unwrap_or_default()
        } else {
            0
        };
        let last_synced_at = if same_source {
            current.and_then(|state| state.last_synced_at)
        } else {
            None
        };
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO system_agent_prompt_sync (
                id, source_instance_id, installed_bundle_version, remote_bundle_version,
                update_available, required, prompt_count, last_checked_at,
                last_synced_at, last_error
            ) VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, NULL)
            ON CONFLICT(id) DO UPDATE SET
                source_instance_id = excluded.source_instance_id,
                installed_bundle_version = excluded.installed_bundle_version,
                remote_bundle_version = excluded.remote_bundle_version,
                update_available = excluded.update_available,
                required = excluded.required,
                prompt_count = excluded.prompt_count,
                last_checked_at = excluded.last_checked_at,
                last_synced_at = excluded.last_synced_at,
                last_error = NULL
            "#,
        )
        .bind(source_instance_id)
        .bind(installed_version)
        .bind(manifest.bundle_version)
        .bind(manifest.bundle_version > installed_version || capability_update_available)
        .bind(manifest.required)
        .bind(prompt_count)
        .bind(now)
        .bind(last_synced_at)
        .execute(self.pool())
        .await
        .context("save local Agent Prompt manifest")?;
        Ok(())
    }

    pub(crate) async fn save_agent_prompt_check_error(
        &self,
        source_instance_id: &str,
        error: &str,
    ) -> Result<()> {
        let current = self.get_agent_prompt_sync_state().await?;
        let same_source = current
            .as_ref()
            .is_some_and(|state| state.source_instance_id == source_instance_id);
        let installed_version = current
            .as_ref()
            .filter(|_| same_source)
            .map(|state| state.installed_bundle_version)
            .unwrap_or_default();
        let remote_version = current
            .as_ref()
            .filter(|_| same_source)
            .map(|state| state.remote_bundle_version)
            .unwrap_or_default();
        let prompt_count = current
            .as_ref()
            .filter(|_| same_source)
            .map(|state| state.prompt_count)
            .unwrap_or_default();
        let required = current
            .as_ref()
            .filter(|_| same_source)
            .is_some_and(|state| state.required);
        let last_synced_at = current
            .filter(|_| same_source)
            .and_then(|state| state.last_synced_at);
        sqlx::query(
            r#"
            INSERT INTO system_agent_prompt_sync (
                id, source_instance_id, installed_bundle_version, remote_bundle_version,
                update_available, required, prompt_count, last_checked_at,
                last_synced_at, last_error
            ) VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                source_instance_id = excluded.source_instance_id,
                installed_bundle_version = excluded.installed_bundle_version,
                remote_bundle_version = excluded.remote_bundle_version,
                update_available = excluded.update_available,
                required = excluded.required,
                prompt_count = excluded.prompt_count,
                last_checked_at = excluded.last_checked_at,
                last_synced_at = excluded.last_synced_at,
                last_error = excluded.last_error
            "#,
        )
        .bind(source_instance_id)
        .bind(installed_version)
        .bind(remote_version)
        .bind(remote_version > installed_version)
        .bind(required)
        .bind(prompt_count)
        .bind(local_now_rfc3339())
        .bind(last_synced_at)
        .bind(error)
        .execute(self.pool())
        .await
        .context("save local Agent Prompt check error")?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn install_agent_prompt_bundle(
        &self,
        source_instance_id: &str,
        bundle: &AgentPromptBundle,
    ) -> Result<()> {
        let required = self
            .get_agent_prompt_sync_state()
            .await?
            .filter(|state| state.source_instance_id == source_instance_id)
            .is_some_and(|state| state.required);
        let synced_at = local_now_rfc3339();
        let mut transaction = self.begin_write().await?;
        sqlx::query("DELETE FROM system_agent_prompts")
            .execute(&mut *transaction)
            .await
            .context("clear previous local Agent Prompt bundle")?;
        for prompt in &bundle.prompts {
            sqlx::query(
                r#"
                INSERT INTO system_agent_prompts (
                    agent_key, vendor, content, revision, checksum, bundle_version,
                    published_at, synced_at, source_instance_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(prompt.agent_key.as_str())
            .bind(prompt.vendor.as_str())
            .bind(prompt.content.as_str())
            .bind(prompt.revision)
            .bind(prompt.checksum.as_str())
            .bind(bundle.bundle_version)
            .bind(prompt.published_at.as_str())
            .bind(synced_at.as_str())
            .bind(source_instance_id)
            .execute(&mut *transaction)
            .await
            .context("insert local Agent Prompt")?;
        }
        sqlx::query(
            r#"
            INSERT INTO system_agent_prompt_sync (
                id, source_instance_id, installed_bundle_version, remote_bundle_version,
                update_available, required, prompt_count, last_checked_at,
                last_synced_at, last_error
            ) VALUES (1, ?, ?, ?, 0, ?, ?, ?, ?, NULL)
            ON CONFLICT(id) DO UPDATE SET
                source_instance_id = excluded.source_instance_id,
                installed_bundle_version = excluded.installed_bundle_version,
                remote_bundle_version = excluded.remote_bundle_version,
                update_available = 0,
                required = excluded.required,
                prompt_count = excluded.prompt_count,
                last_checked_at = excluded.last_checked_at,
                last_synced_at = excluded.last_synced_at,
                last_error = NULL
            "#,
        )
        .bind(source_instance_id)
        .bind(bundle.bundle_version)
        .bind(bundle.bundle_version)
        .bind(required)
        .bind(bundle.prompts.len() as i64)
        .bind(synced_at.as_str())
        .bind(synced_at.as_str())
        .execute(&mut *transaction)
        .await
        .context("save local Agent Prompt sync state")?;
        transaction
            .commit()
            .await
            .context("commit local Agent Prompt bundle")
    }

    pub(crate) async fn install_agent_configuration_bundle(
        &self,
        source_instance_id: &str,
        bundle: &AgentPromptBundle,
        snapshots: &[ResolvedAgentCapabilities],
    ) -> Result<()> {
        let owner_user_id = super::super::capabilities::validate_complete_snapshot_set(snapshots)?;
        let required = self
            .get_agent_prompt_sync_state()
            .await?
            .filter(|state| state.source_instance_id == source_instance_id)
            .is_some_and(|state| state.required);
        let synced_at = local_now_rfc3339();
        let mut transaction = self.begin_write().await?;
        sqlx::query("DELETE FROM system_agent_prompts")
            .execute(&mut *transaction)
            .await
            .context("clear previous local Agent configuration prompts")?;
        for prompt in &bundle.prompts {
            sqlx::query(
                r#"
                INSERT INTO system_agent_prompts (
                    agent_key, vendor, content, revision, checksum, bundle_version,
                    published_at, synced_at, source_instance_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(prompt.agent_key.as_str())
            .bind(prompt.vendor.as_str())
            .bind(prompt.content.as_str())
            .bind(prompt.revision)
            .bind(prompt.checksum.as_str())
            .bind(bundle.bundle_version)
            .bind(prompt.published_at.as_str())
            .bind(synced_at.as_str())
            .bind(source_instance_id)
            .execute(&mut *transaction)
            .await
            .context("insert local Agent configuration prompt")?;
        }

        sqlx::query("DELETE FROM agent_capability_snapshots WHERE owner_user_id = ?")
            .bind(owner_user_id)
            .execute(&mut *transaction)
            .await
            .context("clear previous local Agent capability bundle")?;
        for snapshot in snapshots {
            let generated_at = snapshot.generated_at.trim();
            let generated_at = if generated_at.is_empty() {
                synced_at.as_str()
            } else {
                generated_at
            };
            let payload_json = serde_json::to_string(snapshot)
                .context("encode local Agent capability snapshot")?;
            sqlx::query(
                r#"
                INSERT INTO agent_capability_snapshots (
                    owner_user_id, agent_key, policy_revision, payload_json,
                    generated_at, synced_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(snapshot.owner_user_id.as_str())
            .bind(snapshot.agent_key.as_str())
            .bind(snapshot.policy_revision.as_str())
            .bind(payload_json)
            .bind(generated_at)
            .bind(synced_at.as_str())
            .execute(&mut *transaction)
            .await
            .context("insert local Agent capability snapshot")?;
        }

        sqlx::query(
            r#"
            INSERT INTO system_agent_prompt_sync (
                id, source_instance_id, installed_bundle_version, remote_bundle_version,
                update_available, required, prompt_count, last_checked_at,
                last_synced_at, last_error
            ) VALUES (1, ?, ?, ?, 0, ?, ?, ?, ?, NULL)
            ON CONFLICT(id) DO UPDATE SET
                source_instance_id = excluded.source_instance_id,
                installed_bundle_version = excluded.installed_bundle_version,
                remote_bundle_version = excluded.remote_bundle_version,
                update_available = 0,
                required = excluded.required,
                prompt_count = excluded.prompt_count,
                last_checked_at = excluded.last_checked_at,
                last_synced_at = excluded.last_synced_at,
                last_error = NULL
            "#,
        )
        .bind(source_instance_id)
        .bind(bundle.bundle_version)
        .bind(bundle.bundle_version)
        .bind(required)
        .bind(bundle.prompts.len() as i64)
        .bind(synced_at.as_str())
        .bind(synced_at.as_str())
        .execute(&mut *transaction)
        .await
        .context("save local Agent configuration sync state")?;
        transaction
            .commit()
            .await
            .context("commit local Agent configuration bundle")
    }
}
