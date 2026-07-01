// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_remote_servers(
        &self,
    ) -> Result<Vec<RemoteServerRecord>, String> {
        let rows =
            sqlx::query("SELECT * FROM remote_servers ORDER BY datetime(updated_at) DESC, id DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
        rows.iter().map(remote_server_from_row).collect()
    }

    pub(in crate::store) async fn get_remote_server(
        &self,
        id: &str,
    ) -> Result<Option<RemoteServerRecord>, String> {
        let row = sqlx::query("SELECT * FROM remote_servers WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(remote_server_from_row).transpose()
    }

    pub(in crate::store) async fn save_remote_server(
        &self,
        server: RemoteServerRecord,
    ) -> Result<RemoteServerRecord, String> {
        sqlx::query(
            "INSERT INTO remote_servers (
                id, name, host, port, username, auth_type, password, private_key_path,
                certificate_path, default_remote_path, host_key_policy, enabled,
                last_tested_at, last_test_status, last_test_message, last_active_at,
                creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name, task_id,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                host = excluded.host,
                port = excluded.port,
                username = excluded.username,
                auth_type = excluded.auth_type,
                password = excluded.password,
                private_key_path = excluded.private_key_path,
                certificate_path = excluded.certificate_path,
                default_remote_path = excluded.default_remote_path,
                host_key_policy = excluded.host_key_policy,
                enabled = excluded.enabled,
                last_tested_at = excluded.last_tested_at,
                last_test_status = excluded.last_test_status,
                last_test_message = excluded.last_test_message,
                last_active_at = excluded.last_active_at,
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                task_id = excluded.task_id,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(&server.id)
        .bind(&server.name)
        .bind(&server.host)
        .bind(server.port)
        .bind(&server.username)
        .bind(&server.auth_type)
        .bind(server.password.clone())
        .bind(server.private_key_path.clone())
        .bind(server.certificate_path.clone())
        .bind(server.default_remote_path.clone())
        .bind(&server.host_key_policy)
        .bind(bool_to_int(server.enabled))
        .bind(server.last_tested_at.clone())
        .bind(server.last_test_status.clone())
        .bind(server.last_test_message.clone())
        .bind(server.last_active_at.clone())
        .bind(server.creator_user_id.clone())
        .bind(server.creator_username.clone())
        .bind(server.creator_display_name.clone())
        .bind(server.owner_user_id.clone())
        .bind(server.owner_username.clone())
        .bind(server.owner_display_name.clone())
        .bind(server.task_id.clone())
        .bind(&server.created_at)
        .bind(&server.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(server)
    }

    pub(in crate::store) async fn delete_remote_server(&self, id: &str) -> Result<bool, String> {
        let result = sqlx::query("DELETE FROM remote_servers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }
}
