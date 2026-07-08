// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::models::{
    bool_to_int, capabilities_to_json, now_rfc3339, LocalConnectorDevice, LocalConnectorDeviceRow,
    LocalConnectorProjectBinding, LocalConnectorProjectBindingRow, LocalConnectorSandboxPairing,
    LocalConnectorSandboxPairingRow, LocalConnectorSession, LocalConnectorWorkspace,
    LocalConnectorWorkspaceRow, DEVICE_STATUS_OFFLINE, DEVICE_STATUS_REVOKED,
    SESSION_STATUS_CONNECTED, SESSION_STATUS_DISCONNECTED,
};

#[derive(Clone)]
pub struct ConnectorStore {
    pool: SqlitePool,
}

impl ConnectorStore {
    pub async fn connect(database_url: &str) -> Result<Self, String> {
        ensure_sqlite_parent_dir(database_url)?;
        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(|err| format!("parse local connector database url failed: {err}"))?
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .map_err(|err| format!("connect local connector database failed: {err}"))?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|err| format!("run local connector migrations failed: {err}"))?;
        Ok(Self { pool })
    }

    pub async fn create_device(&self, device: &LocalConnectorDevice) -> Result<(), String> {
        sqlx::query("INSERT INTO local_connector_devices (id, owner_user_id, display_name, public_key, client_version, os, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&device.id)
            .bind(&device.owner_user_id)
            .bind(&device.display_name)
            .bind(&device.public_key)
            .bind(&device.client_version)
            .bind(&device.os)
            .bind(&device.status)
            .bind(&device.created_at)
            .bind(&device.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_device(&self, id: &str) -> Result<Option<LocalConnectorDevice>, String> {
        let row = sqlx::query_as::<_, LocalConnectorDeviceRow>(
            "SELECT * FROM local_connector_devices WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(row.map(LocalConnectorDeviceRow::into_model))
    }

    pub async fn list_devices(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalConnectorDevice>, String> {
        let rows = sqlx::query_as::<_, LocalConnectorDeviceRow>(
            "SELECT * FROM local_connector_devices WHERE owner_user_id = ? ORDER BY updated_at DESC",
        )
        .bind(owner_user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows
            .into_iter()
            .map(LocalConnectorDeviceRow::into_model)
            .collect())
    }

    pub async fn mark_device_online(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE local_connector_devices SET status = 'online', last_seen_at = ?, updated_at = ? WHERE id = ? AND status != ?")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .bind(DEVICE_STATUS_REVOKED)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn mark_device_offline(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE local_connector_devices SET status = ?, updated_at = ? WHERE id = ? AND status != ?")
            .bind(DEVICE_STATUS_OFFLINE)
            .bind(&now)
            .bind(id)
            .bind(DEVICE_STATUS_REVOKED)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn revoke_device(&self, owner_user_id: &str, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE local_connector_devices SET status = ?, revoked_at = ?, updated_at = ? WHERE id = ? AND owner_user_id = ?")
            .bind(DEVICE_STATUS_REVOKED)
            .bind(&now)
            .bind(&now)
            .bind(id)
            .bind(owner_user_id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn create_workspace(
        &self,
        workspace: &LocalConnectorWorkspace,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO local_connector_workspaces (id, owner_user_id, device_id, display_name, local_path_alias, local_path_fingerprint, capabilities_json, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&workspace.id)
            .bind(&workspace.owner_user_id)
            .bind(&workspace.device_id)
            .bind(&workspace.display_name)
            .bind(&workspace.local_path_alias)
            .bind(&workspace.local_path_fingerprint)
            .bind(capabilities_to_json(&workspace.capabilities))
            .bind(&workspace.status)
            .bind(&workspace.created_at)
            .bind(&workspace.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_workspace(&self, id: &str) -> Result<Option<LocalConnectorWorkspace>, String> {
        let row = sqlx::query_as::<_, LocalConnectorWorkspaceRow>(
            "SELECT * FROM local_connector_workspaces WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(row.map(LocalConnectorWorkspaceRow::into_model))
    }

    pub async fn list_workspaces(
        &self,
        owner_user_id: &str,
        device_id: Option<String>,
    ) -> Result<Vec<LocalConnectorWorkspace>, String> {
        let rows = if let Some(device_id) = device_id {
            sqlx::query_as::<_, LocalConnectorWorkspaceRow>(
                "SELECT * FROM local_connector_workspaces WHERE owner_user_id = ? AND device_id = ? ORDER BY updated_at DESC",
            )
            .bind(owner_user_id)
            .bind(device_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?
        } else {
            sqlx::query_as::<_, LocalConnectorWorkspaceRow>(
                "SELECT * FROM local_connector_workspaces WHERE owner_user_id = ? ORDER BY updated_at DESC",
            )
            .bind(owner_user_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?
        };
        Ok(rows
            .into_iter()
            .map(LocalConnectorWorkspaceRow::into_model)
            .collect())
    }

    pub async fn update_workspace(
        &self,
        workspace: &LocalConnectorWorkspace,
    ) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE local_connector_workspaces SET device_id = ?, display_name = ?, local_path_alias = ?, local_path_fingerprint = ?, capabilities_json = ?, status = ?, updated_at = ? WHERE id = ? AND owner_user_id = ?")
            .bind(&workspace.device_id)
            .bind(&workspace.display_name)
            .bind(&workspace.local_path_alias)
            .bind(&workspace.local_path_fingerprint)
            .bind(capabilities_to_json(&workspace.capabilities))
            .bind(&workspace.status)
            .bind(&now)
            .bind(&workspace.id)
            .bind(&workspace.owner_user_id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_workspace(&self, owner_user_id: &str, id: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM local_connector_workspaces WHERE id = ? AND owner_user_id = ?")
            .bind(id)
            .bind(owner_user_id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn upsert_project_binding(
        &self,
        binding: &LocalConnectorProjectBinding,
    ) -> Result<LocalConnectorProjectBinding, String> {
        let now = now_rfc3339();
        sqlx::query("INSERT INTO local_connector_project_bindings (id, owner_user_id, project_id, device_id, workspace_id, mode, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(owner_user_id, project_id, mode) DO UPDATE SET device_id = excluded.device_id, workspace_id = excluded.workspace_id, enabled = excluded.enabled, updated_at = excluded.updated_at")
            .bind(&binding.id)
            .bind(&binding.owner_user_id)
            .bind(&binding.project_id)
            .bind(&binding.device_id)
            .bind(&binding.workspace_id)
            .bind(&binding.mode)
            .bind(bool_to_int(binding.enabled))
            .bind(&binding.created_at)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        self.get_project_binding_by_scope(
            binding.owner_user_id.as_str(),
            binding.project_id.as_str(),
            binding.mode.as_str(),
        )
        .await?
        .ok_or_else(|| "project binding not found after upsert".to_string())
    }

    pub async fn get_project_binding(
        &self,
        id: &str,
    ) -> Result<Option<LocalConnectorProjectBinding>, String> {
        let row = sqlx::query_as::<_, LocalConnectorProjectBindingRow>(
            "SELECT * FROM local_connector_project_bindings WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(row.map(LocalConnectorProjectBindingRow::into_model))
    }

    async fn get_project_binding_by_scope(
        &self,
        owner_user_id: &str,
        project_id: &str,
        mode: &str,
    ) -> Result<Option<LocalConnectorProjectBinding>, String> {
        let row = sqlx::query_as::<_, LocalConnectorProjectBindingRow>("SELECT * FROM local_connector_project_bindings WHERE owner_user_id = ? AND project_id = ? AND mode = ? LIMIT 1")
            .bind(owner_user_id)
            .bind(project_id)
            .bind(mode)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.map(LocalConnectorProjectBindingRow::into_model))
    }

    pub async fn list_project_bindings(
        &self,
        owner_user_id: &str,
        project_id: Option<String>,
        mode: Option<String>,
    ) -> Result<Vec<LocalConnectorProjectBinding>, String> {
        let rows = match (project_id, mode) {
            (Some(project_id), Some(mode)) => {
                sqlx::query_as::<_, LocalConnectorProjectBindingRow>("SELECT * FROM local_connector_project_bindings WHERE owner_user_id = ? AND project_id = ? AND mode = ? ORDER BY updated_at DESC")
                    .bind(owner_user_id)
                    .bind(project_id)
                    .bind(mode)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|err| err.to_string())?
            }
            (Some(project_id), None) => {
                sqlx::query_as::<_, LocalConnectorProjectBindingRow>("SELECT * FROM local_connector_project_bindings WHERE owner_user_id = ? AND project_id = ? ORDER BY updated_at DESC")
                    .bind(owner_user_id)
                    .bind(project_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|err| err.to_string())?
            }
            (None, Some(mode)) => {
                sqlx::query_as::<_, LocalConnectorProjectBindingRow>("SELECT * FROM local_connector_project_bindings WHERE owner_user_id = ? AND mode = ? ORDER BY updated_at DESC")
                    .bind(owner_user_id)
                    .bind(mode)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|err| err.to_string())?
            }
            (None, None) => {
                sqlx::query_as::<_, LocalConnectorProjectBindingRow>("SELECT * FROM local_connector_project_bindings WHERE owner_user_id = ? ORDER BY updated_at DESC")
                    .bind(owner_user_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|err| err.to_string())?
            }
        };
        Ok(rows
            .into_iter()
            .map(LocalConnectorProjectBindingRow::into_model)
            .collect())
    }

    pub async fn update_project_binding(
        &self,
        binding: &LocalConnectorProjectBinding,
    ) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE local_connector_project_bindings SET device_id = ?, workspace_id = ?, enabled = ?, updated_at = ? WHERE id = ? AND owner_user_id = ?")
            .bind(&binding.device_id)
            .bind(&binding.workspace_id)
            .bind(bool_to_int(binding.enabled))
            .bind(&now)
            .bind(&binding.id)
            .bind(&binding.owner_user_id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_project_binding(
        &self,
        owner_user_id: &str,
        id: &str,
    ) -> Result<(), String> {
        sqlx::query(
            "DELETE FROM local_connector_project_bindings WHERE id = ? AND owner_user_id = ?",
        )
        .bind(id)
        .bind(owner_user_id)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn upsert_sandbox_pairing(
        &self,
        pairing: &LocalConnectorSandboxPairing,
    ) -> Result<LocalConnectorSandboxPairing, String> {
        let now = now_rfc3339();
        sqlx::query("INSERT INTO local_connector_sandbox_pairings (id, owner_user_id, device_id, workspace_id, enabled, sandbox_mode, facade_base_url, access_client_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(owner_user_id, device_id, workspace_id) DO UPDATE SET enabled = excluded.enabled, sandbox_mode = excluded.sandbox_mode, facade_base_url = excluded.facade_base_url, access_client_id = excluded.access_client_id, updated_at = excluded.updated_at")
            .bind(&pairing.id)
            .bind(&pairing.owner_user_id)
            .bind(&pairing.device_id)
            .bind(&pairing.workspace_id)
            .bind(bool_to_int(pairing.enabled))
            .bind(&pairing.sandbox_mode)
            .bind(&pairing.facade_base_url)
            .bind(&pairing.access_client_id)
            .bind(&pairing.created_at)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        self.get_sandbox_pairing_by_scope(
            pairing.owner_user_id.as_str(),
            pairing.device_id.as_str(),
            pairing.workspace_id.as_str(),
        )
        .await?
        .ok_or_else(|| "sandbox pairing not found after upsert".to_string())
    }

    pub async fn get_sandbox_pairing(
        &self,
        id: &str,
    ) -> Result<Option<LocalConnectorSandboxPairing>, String> {
        let row = sqlx::query_as::<_, LocalConnectorSandboxPairingRow>(
            "SELECT * FROM local_connector_sandbox_pairings WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(row.map(LocalConnectorSandboxPairingRow::into_model))
    }

    async fn get_sandbox_pairing_by_scope(
        &self,
        owner_user_id: &str,
        device_id: &str,
        workspace_id: &str,
    ) -> Result<Option<LocalConnectorSandboxPairing>, String> {
        let row = sqlx::query_as::<_, LocalConnectorSandboxPairingRow>("SELECT * FROM local_connector_sandbox_pairings WHERE owner_user_id = ? AND device_id = ? AND workspace_id = ? LIMIT 1")
            .bind(owner_user_id)
            .bind(device_id)
            .bind(workspace_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.map(LocalConnectorSandboxPairingRow::into_model))
    }

    pub async fn list_sandbox_pairings(
        &self,
        owner_user_id: &str,
        device_id: Option<String>,
        workspace_id: Option<String>,
    ) -> Result<Vec<LocalConnectorSandboxPairing>, String> {
        let rows = match (device_id, workspace_id) {
            (Some(device_id), Some(workspace_id)) => {
                sqlx::query_as::<_, LocalConnectorSandboxPairingRow>("SELECT * FROM local_connector_sandbox_pairings WHERE owner_user_id = ? AND device_id = ? AND workspace_id = ? ORDER BY updated_at DESC")
                    .bind(owner_user_id)
                    .bind(device_id)
                    .bind(workspace_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|err| err.to_string())?
            }
            (Some(device_id), None) => {
                sqlx::query_as::<_, LocalConnectorSandboxPairingRow>("SELECT * FROM local_connector_sandbox_pairings WHERE owner_user_id = ? AND device_id = ? ORDER BY updated_at DESC")
                    .bind(owner_user_id)
                    .bind(device_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|err| err.to_string())?
            }
            (None, Some(workspace_id)) => {
                sqlx::query_as::<_, LocalConnectorSandboxPairingRow>("SELECT * FROM local_connector_sandbox_pairings WHERE owner_user_id = ? AND workspace_id = ? ORDER BY updated_at DESC")
                    .bind(owner_user_id)
                    .bind(workspace_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|err| err.to_string())?
            }
            (None, None) => {
                sqlx::query_as::<_, LocalConnectorSandboxPairingRow>("SELECT * FROM local_connector_sandbox_pairings WHERE owner_user_id = ? ORDER BY updated_at DESC")
                    .bind(owner_user_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|err| err.to_string())?
            }
        };
        Ok(rows
            .into_iter()
            .map(LocalConnectorSandboxPairingRow::into_model)
            .collect())
    }

    pub async fn update_sandbox_pairing(
        &self,
        pairing: &LocalConnectorSandboxPairing,
    ) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE local_connector_sandbox_pairings SET workspace_id = ?, enabled = ?, sandbox_mode = ?, facade_base_url = ?, access_client_id = ?, updated_at = ? WHERE id = ? AND owner_user_id = ?")
            .bind(&pairing.workspace_id)
            .bind(bool_to_int(pairing.enabled))
            .bind(&pairing.sandbox_mode)
            .bind(&pairing.facade_base_url)
            .bind(&pairing.access_client_id)
            .bind(&now)
            .bind(&pairing.id)
            .bind(&pairing.owner_user_id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_sandbox_pairing(
        &self,
        owner_user_id: &str,
        id: &str,
    ) -> Result<(), String> {
        sqlx::query(
            "DELETE FROM local_connector_sandbox_pairings WHERE id = ? AND owner_user_id = ?",
        )
        .bind(id)
        .bind(owner_user_id)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn open_session(&self, session: &LocalConnectorSession) -> Result<(), String> {
        sqlx::query("INSERT INTO local_connector_sessions (id, owner_user_id, device_id, connection_id, status, connected_at, last_heartbeat_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&session.id)
            .bind(&session.owner_user_id)
            .bind(&session.device_id)
            .bind(&session.connection_id)
            .bind(&session.status)
            .bind(&session.connected_at)
            .bind(&session.last_heartbeat_at)
            .bind(&session.created_at)
            .bind(&session.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn heartbeat_session(&self, session_id: &str, device_id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE local_connector_sessions SET status = ?, last_heartbeat_at = ?, updated_at = ? WHERE id = ?")
            .bind(SESSION_STATUS_CONNECTED)
            .bind(&now)
            .bind(&now)
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::query("UPDATE local_connector_devices SET status = 'online', last_seen_at = ?, updated_at = ? WHERE id = ? AND status != ?")
            .bind(&now)
            .bind(&now)
            .bind(device_id)
            .bind(DEVICE_STATUS_REVOKED)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn close_session(&self, session_id: &str, device_id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE local_connector_sessions SET status = ?, disconnected_at = ?, updated_at = ? WHERE id = ?")
            .bind(SESSION_STATUS_DISCONNECTED)
            .bind(&now)
            .bind(&now)
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        self.mark_device_offline(device_id).await
    }
}

fn ensure_sqlite_parent_dir(database_url: &str) -> Result<(), String> {
    let Some(path) = sqlite_file_path(database_url) else {
        return Ok(());
    };
    let Some(parent) = Path::new(path.as_str()).parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("create local connector database dir failed: {err}"))
}

fn sqlite_file_path(database_url: &str) -> Option<String> {
    let trimmed = database_url.trim();
    if trimmed == "sqlite::memory:" || trimmed == "sqlite://:memory:" {
        return None;
    }
    trimmed
        .strip_prefix("sqlite://")
        .or_else(|| trimmed.strip_prefix("sqlite:"))
        .map(ToOwned::to_owned)
        .filter(|value| !value.is_empty())
}
