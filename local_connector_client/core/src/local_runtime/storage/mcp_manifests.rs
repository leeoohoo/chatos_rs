// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::mcp::manifest::LocalMcpManifestRecord;

use super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn list_mcp_manifests(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<Vec<LocalMcpManifestRecord>> {
        let payloads = sqlx::query_scalar::<_, String>(
            r#"
            SELECT payload_json FROM local_mcp_manifests
            WHERE owner_user_id = ? AND device_id = ?
            ORDER BY updated_at DESC, manifest_id ASC
            "#,
        )
        .bind(owner_user_id)
        .bind(device_id)
        .fetch_all(self.pool())
        .await
        .context("list local MCP manifests")?;
        payloads
            .into_iter()
            .map(|payload| decode_manifest(payload.as_str(), owner_user_id, device_id))
            .collect()
    }

    pub(crate) async fn get_mcp_manifest(
        &self,
        owner_user_id: &str,
        device_id: &str,
        manifest_id: &str,
    ) -> Result<Option<LocalMcpManifestRecord>> {
        let payload = sqlx::query_scalar::<_, String>(
            r#"
            SELECT payload_json FROM local_mcp_manifests
            WHERE owner_user_id = ? AND device_id = ? AND manifest_id = ?
            "#,
        )
        .bind(owner_user_id)
        .bind(device_id)
        .bind(manifest_id)
        .fetch_optional(self.pool())
        .await
        .context("get local MCP manifest")?;
        payload
            .map(|payload| decode_manifest(payload.as_str(), owner_user_id, device_id))
            .transpose()
    }

    pub(crate) async fn save_mcp_manifest(&self, record: &LocalMcpManifestRecord) -> Result<()> {
        let payload = serde_json::to_string(record).context("encode local MCP manifest")?;
        sqlx::query(
            r#"
            INSERT INTO local_mcp_manifests (
                manifest_id, owner_user_id, device_id, plugin_mcp_id,
                enabled, sync_status, last_check_status, manifest_hash,
                payload_json, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(owner_user_id, device_id, manifest_id) DO UPDATE SET
                plugin_mcp_id = excluded.plugin_mcp_id,
                enabled = excluded.enabled,
                sync_status = excluded.sync_status,
                last_check_status = excluded.last_check_status,
                manifest_hash = excluded.manifest_hash,
                payload_json = excluded.payload_json,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(record.manifest_id.as_str())
        .bind(record.owner_user_id.as_str())
        .bind(record.device_id.as_str())
        .bind(record.plugin_mcp_id.as_deref())
        .bind(record.enabled)
        .bind(record.sync_status.as_str())
        .bind(record.last_check_status.as_str())
        .bind(record.manifest_hash.as_str())
        .bind(payload)
        .bind(record.created_at.as_str())
        .bind(record.updated_at.as_str())
        .execute(self.pool())
        .await
        .context("save local MCP manifest")?;
        Ok(())
    }

    pub(crate) async fn delete_mcp_manifest(
        &self,
        owner_user_id: &str,
        device_id: &str,
        manifest_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM local_mcp_manifests WHERE owner_user_id = ? AND device_id = ? AND manifest_id = ?",
        )
        .bind(owner_user_id)
        .bind(device_id)
        .bind(manifest_id)
        .execute(self.pool())
        .await
        .context("delete local MCP manifest")?;
        Ok(result.rows_affected() > 0)
    }
}

fn decode_manifest(
    payload: &str,
    owner_user_id: &str,
    device_id: &str,
) -> Result<LocalMcpManifestRecord> {
    let record = serde_json::from_str::<LocalMcpManifestRecord>(payload)
        .context("decode local MCP manifest")?;
    if record.owner_user_id != owner_user_id || record.device_id != device_id {
        return Err(anyhow::anyhow!(
            "local MCP manifest identity does not match its SQLite key"
        ));
    }
    Ok(record)
}

#[cfg(test)]
#[path = "mcp_manifests_tests.rs"]
mod tests;
