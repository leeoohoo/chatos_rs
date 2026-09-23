// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    now_rfc3339, LocalConnectorDevice, LocalConnectorProjectBinding, LocalConnectorSandboxPairing,
    LocalConnectorStoreStats, LocalConnectorWorkspace, DEVICE_STATUS_OFFLINE, DEVICE_STATUS_ONLINE,
    DEVICE_STATUS_REVOKED,
};

use super::{db_error, decode_all, decode_optional, json, timestamp, ConnectorStore};

mod managed_requirements;
mod sessions;

impl ConnectorStore {
    pub async fn register_device(
        &self,
        device: &LocalConnectorDevice,
    ) -> Result<(LocalConnectorDevice, bool), String> {
        let value = sqlx::query_scalar(
            "INSERT INTO local_connector_devices(id,owner_user_id,public_key,status,updated_at,data)
             VALUES($1,$2,$3,$4,$5,$6)
             ON CONFLICT(owner_user_id,public_key)
               WHERE status IN ('registered','online','offline')
             DO UPDATE SET updated_at=EXCLUDED.updated_at,
               data=local_connector_devices.data || jsonb_build_object(
                 'display_name',EXCLUDED.data->'display_name',
                 'client_version',EXCLUDED.data->'client_version','os',EXCLUDED.data->'os',
                 'updated_at',EXCLUDED.data->'updated_at')
             RETURNING data",
        )
        .bind(&device.id)
        .bind(&device.owner_user_id)
        .bind(&device.public_key)
        .bind(&device.status)
        .bind(timestamp(&device.updated_at)?)
        .bind(json(device)?)
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)?;
        let registered: LocalConnectorDevice = decode_optional(Some(value))?
            .ok_or_else(|| "device registration did not return a record".to_string())?;
        let created = registered.id == device.id;
        Ok((registered, created))
    }

    pub async fn get_device(&self, id: &str) -> Result<Option<LocalConnectorDevice>, String> {
        decode_optional(
            sqlx::query_scalar("SELECT data FROM local_connector_devices WHERE id=$1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_error)?,
        )
    }

    pub async fn register_device_windows_user_sid(
        &self,
        owner_user_id: &str,
        id: &str,
        windows_user_sid: &str,
    ) -> Result<bool, String> {
        let now = now_rfc3339();
        sqlx::query(
            "UPDATE local_connector_devices SET updated_at=$4,
             data=data || jsonb_build_object('windows_user_sid',$3::text,'updated_at',$5::text)
             WHERE id=$1 AND owner_user_id=$2 AND status<>$6
               AND (data->>'windows_user_sid' IS NULL OR data->>'windows_user_sid'=$3)",
        )
        .bind(id)
        .bind(owner_user_id)
        .bind(windows_user_sid)
        .bind(timestamp(&now)?)
        .bind(&now)
        .bind(DEVICE_STATUS_REVOKED)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(db_error)
    }

    pub async fn list_devices(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalConnectorDevice>, String> {
        self.cleanup_expired_owner_session(owner_user_id).await?;
        decode_all(
            sqlx::query_scalar(
                "SELECT data FROM local_connector_devices WHERE owner_user_id=$1 ORDER BY updated_at DESC",
            )
            .bind(owner_user_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn mark_device_online(&self, id: &str) -> Result<(), String> {
        self.set_device_status(id, DEVICE_STATUS_ONLINE, true).await
    }

    pub async fn mark_device_offline(&self, id: &str) -> Result<(), String> {
        self.set_device_status(id, DEVICE_STATUS_OFFLINE, false)
            .await
    }

    async fn set_device_status(
        &self,
        id: &str,
        status: &str,
        update_last_seen: bool,
    ) -> Result<(), String> {
        let now = now_rfc3339();
        let patch = if update_last_seen {
            serde_json::json!({"status": status, "last_seen_at": now, "updated_at": now})
        } else {
            serde_json::json!({"status": status, "updated_at": now})
        };
        sqlx::query(
            "UPDATE local_connector_devices SET status=$2,updated_at=$3,data=data || $4
             WHERE id=$1 AND status<>$5",
        )
        .bind(id)
        .bind(status)
        .bind(timestamp(&now)?)
        .bind(sqlx::types::Json(patch))
        .bind(DEVICE_STATUS_REVOKED)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn revoke_device(&self, owner_user_id: &str, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query(
            "UPDATE local_connector_devices SET status=$3,updated_at=$4,
             data=data || jsonb_build_object('status',$3::text,'revoked_at',$5::text,'updated_at',$5::text)
             WHERE id=$1 AND owner_user_id=$2",
        )
        .bind(id)
        .bind(owner_user_id)
        .bind(DEVICE_STATUS_REVOKED)
        .bind(timestamp(&now)?)
        .bind(now)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn create_workspace(
        &self,
        workspace: &LocalConnectorWorkspace,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO local_connector_workspaces(id,owner_user_id,device_id,status,updated_at,data) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(&workspace.id).bind(&workspace.owner_user_id).bind(&workspace.device_id)
            .bind(&workspace.status).bind(timestamp(&workspace.updated_at)?).bind(json(workspace)?)
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn get_workspace(&self, id: &str) -> Result<Option<LocalConnectorWorkspace>, String> {
        decode_optional(
            sqlx::query_scalar("SELECT data FROM local_connector_workspaces WHERE id=$1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_error)?,
        )
    }

    pub async fn list_workspaces(
        &self,
        owner_user_id: &str,
        device_id: Option<String>,
    ) -> Result<Vec<LocalConnectorWorkspace>, String> {
        decode_all(sqlx::query_scalar(
            "SELECT data FROM local_connector_workspaces WHERE owner_user_id=$1 AND ($2::text IS NULL OR device_id=$2) ORDER BY updated_at DESC",
        ).bind(owner_user_id).bind(device_id).fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn update_workspace(
        &self,
        workspace: &LocalConnectorWorkspace,
    ) -> Result<(), String> {
        let mut updated = workspace.clone();
        updated.updated_at = now_rfc3339();
        sqlx::query("UPDATE local_connector_workspaces SET device_id=$3,status=$4,updated_at=$5,data=$6 WHERE id=$1 AND owner_user_id=$2")
            .bind(&updated.id).bind(&updated.owner_user_id).bind(&updated.device_id).bind(&updated.status)
            .bind(timestamp(&updated.updated_at)?).bind(json(&updated)?).execute(&self.pool).await
            .map(|_| ()).map_err(db_error)
    }

    pub async fn delete_workspace(&self, owner_user_id: &str, id: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM local_connector_workspaces WHERE id=$1 AND owner_user_id=$2")
            .bind(id)
            .bind(owner_user_id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error)
    }

    pub async fn upsert_project_binding(
        &self,
        binding: &LocalConnectorProjectBinding,
    ) -> Result<LocalConnectorProjectBinding, String> {
        let mut updated = binding.clone();
        updated.updated_at = now_rfc3339();
        let value = sqlx::query_scalar(
            "INSERT INTO local_connector_project_bindings(id,owner_user_id,project_id,mode,device_id,workspace_id,enabled,updated_at,data)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT(owner_user_id,project_id,mode) DO UPDATE SET device_id=EXCLUDED.device_id,
               workspace_id=EXCLUDED.workspace_id,enabled=EXCLUDED.enabled,updated_at=EXCLUDED.updated_at,
               data=local_connector_project_bindings.data || jsonb_build_object(
                 'device_id',EXCLUDED.data->'device_id','workspace_id',EXCLUDED.data->'workspace_id',
                 'enabled',EXCLUDED.data->'enabled','updated_at',EXCLUDED.data->'updated_at')
             RETURNING data",
        ).bind(&updated.id).bind(&updated.owner_user_id).bind(&updated.project_id).bind(&updated.mode)
            .bind(&updated.device_id).bind(&updated.workspace_id).bind(updated.enabled)
            .bind(timestamp(&updated.updated_at)?).bind(json(&updated)?).fetch_one(&self.pool).await.map_err(db_error)?;
        decode_optional(Some(value))?
            .ok_or_else(|| "project binding not found after upsert".to_string())
    }

    pub async fn get_project_binding(
        &self,
        id: &str,
    ) -> Result<Option<LocalConnectorProjectBinding>, String> {
        decode_optional(
            sqlx::query_scalar("SELECT data FROM local_connector_project_bindings WHERE id=$1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_error)?,
        )
    }

    pub async fn list_project_bindings(
        &self,
        owner_user_id: &str,
        project_id: Option<String>,
        mode: Option<String>,
    ) -> Result<Vec<LocalConnectorProjectBinding>, String> {
        decode_all(sqlx::query_scalar(
            "SELECT data FROM local_connector_project_bindings WHERE owner_user_id=$1
             AND ($2::text IS NULL OR project_id=$2) AND ($3::text IS NULL OR mode=$3) ORDER BY updated_at DESC",
        ).bind(owner_user_id).bind(project_id).bind(mode).fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn update_project_binding(
        &self,
        binding: &LocalConnectorProjectBinding,
    ) -> Result<(), String> {
        let mut updated = binding.clone();
        updated.updated_at = now_rfc3339();
        sqlx::query("UPDATE local_connector_project_bindings SET device_id=$3,workspace_id=$4,enabled=$5,updated_at=$6,data=$7 WHERE id=$1 AND owner_user_id=$2")
            .bind(&updated.id).bind(&updated.owner_user_id).bind(&updated.device_id).bind(&updated.workspace_id)
            .bind(updated.enabled).bind(timestamp(&updated.updated_at)?).bind(json(&updated)?)
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn delete_project_binding(
        &self,
        owner_user_id: &str,
        id: &str,
    ) -> Result<(), String> {
        sqlx::query("DELETE FROM local_connector_project_bindings WHERE id=$1 AND owner_user_id=$2")
            .bind(id)
            .bind(owner_user_id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error)
    }

    pub async fn upsert_sandbox_pairing(
        &self,
        pairing: &LocalConnectorSandboxPairing,
    ) -> Result<LocalConnectorSandboxPairing, String> {
        let mut updated = pairing.clone();
        updated.updated_at = now_rfc3339();
        let value = sqlx::query_scalar(
            "INSERT INTO local_connector_sandbox_pairings(id,owner_user_id,device_id,workspace_id,enabled,updated_at,data)
             VALUES($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT(owner_user_id,device_id,workspace_id) DO UPDATE SET enabled=EXCLUDED.enabled,
               updated_at=EXCLUDED.updated_at,data=local_connector_sandbox_pairings.data ||
               (EXCLUDED.data - 'id' - 'created_at') RETURNING data",
        ).bind(&updated.id).bind(&updated.owner_user_id).bind(&updated.device_id).bind(&updated.workspace_id)
            .bind(updated.enabled).bind(timestamp(&updated.updated_at)?).bind(json(&updated)?)
            .fetch_one(&self.pool).await.map_err(db_error)?;
        decode_optional(Some(value))?
            .ok_or_else(|| "sandbox pairing not found after upsert".to_string())
    }

    pub async fn get_sandbox_pairing(
        &self,
        id: &str,
    ) -> Result<Option<LocalConnectorSandboxPairing>, String> {
        decode_optional(
            sqlx::query_scalar("SELECT data FROM local_connector_sandbox_pairings WHERE id=$1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_error)?,
        )
    }

    pub async fn list_sandbox_pairings(
        &self,
        owner_user_id: &str,
        device_id: Option<String>,
        workspace_id: Option<String>,
    ) -> Result<Vec<LocalConnectorSandboxPairing>, String> {
        decode_all(sqlx::query_scalar(
            "SELECT data FROM local_connector_sandbox_pairings WHERE owner_user_id=$1
             AND ($2::text IS NULL OR device_id=$2) AND ($3::text IS NULL OR workspace_id=$3) ORDER BY updated_at DESC",
        ).bind(owner_user_id).bind(device_id).bind(workspace_id).fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn update_sandbox_pairing(
        &self,
        pairing: &LocalConnectorSandboxPairing,
    ) -> Result<(), String> {
        let mut updated = pairing.clone();
        updated.updated_at = now_rfc3339();
        sqlx::query("UPDATE local_connector_sandbox_pairings SET workspace_id=$3,enabled=$4,updated_at=$5,data=$6 WHERE id=$1 AND owner_user_id=$2")
            .bind(&updated.id).bind(&updated.owner_user_id).bind(&updated.workspace_id).bind(updated.enabled)
            .bind(timestamp(&updated.updated_at)?).bind(json(&updated)?).execute(&self.pool).await
            .map(|_| ()).map_err(db_error)
    }

    pub async fn delete_sandbox_pairing(
        &self,
        owner_user_id: &str,
        id: &str,
    ) -> Result<(), String> {
        sqlx::query("DELETE FROM local_connector_sandbox_pairings WHERE id=$1 AND owner_user_id=$2")
            .bind(id)
            .bind(owner_user_id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error)
    }

    pub async fn system_stats(&self) -> Result<LocalConnectorStoreStats, String> {
        let row = sqlx::query_as::<_, (i64,i64,i64,i64,i64,i64,i64,i64)>(
            "SELECT
             (SELECT count(*) FROM local_connector_devices),
             (SELECT count(*) FROM local_connector_devices WHERE status='online'),
             (SELECT count(*) FROM local_connector_devices WHERE status='offline'),
             (SELECT count(*) FROM local_connector_devices WHERE status='revoked'),
             (SELECT count(*) FROM local_connector_workspaces),
             (SELECT count(*) FROM local_connector_project_bindings),
             (SELECT count(*) FROM local_connector_sandbox_pairings),
             (SELECT count(*) FROM local_connector_active_sessions WHERE status='connected' AND expires_at>now())",
        ).fetch_one(&self.pool).await.map_err(db_error)?;
        Ok(LocalConnectorStoreStats {
            devices_total: usize::try_from(row.0).unwrap_or(usize::MAX),
            devices_online: usize::try_from(row.1).unwrap_or(usize::MAX),
            devices_offline: usize::try_from(row.2).unwrap_or(usize::MAX),
            devices_revoked: usize::try_from(row.3).unwrap_or(usize::MAX),
            workspaces_total: usize::try_from(row.4).unwrap_or(usize::MAX),
            project_bindings_total: usize::try_from(row.5).unwrap_or(usize::MAX),
            sandbox_pairings_total: usize::try_from(row.6).unwrap_or(usize::MAX),
            active_sessions_total: usize::try_from(row.7).unwrap_or(usize::MAX),
        })
    }
}
