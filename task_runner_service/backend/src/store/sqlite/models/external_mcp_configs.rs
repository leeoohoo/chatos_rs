// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_external_mcp_configs(
        &self,
    ) -> Result<Vec<ExternalMcpConfigRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM external_mcp_configs ORDER BY datetime(updated_at) DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        rows.iter().map(external_mcp_config_from_row).collect()
    }

    pub(in crate::store) async fn get_external_mcp_config(
        &self,
        id: &str,
    ) -> Result<Option<ExternalMcpConfigRecord>, String> {
        let row = sqlx::query("SELECT * FROM external_mcp_configs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(external_mcp_config_from_row).transpose()
    }

    pub(in crate::store) async fn save_external_mcp_config(
        &self,
        config: ExternalMcpConfigRecord,
    ) -> Result<ExternalMcpConfigRecord, String> {
        sqlx::query(
            "INSERT INTO external_mcp_configs (
                id, name, transport, command, args_json, url, headers_json, env_json, cwd,
                enabled, creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                transport = excluded.transport,
                command = excluded.command,
                args_json = excluded.args_json,
                url = excluded.url,
                headers_json = excluded.headers_json,
                env_json = excluded.env_json,
                cwd = excluded.cwd,
                enabled = excluded.enabled,
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(&config.id)
        .bind(&config.name)
        .bind(&config.transport)
        .bind(config.command.clone())
        .bind(encode_json(&config.args)?)
        .bind(config.url.clone())
        .bind(encode_json(&config.headers)?)
        .bind(encode_json(&config.env)?)
        .bind(config.cwd.clone())
        .bind(bool_to_int(config.enabled))
        .bind(config.creator_user_id.clone())
        .bind(config.creator_username.clone())
        .bind(config.creator_display_name.clone())
        .bind(config.owner_user_id.clone())
        .bind(config.owner_username.clone())
        .bind(config.owner_display_name.clone())
        .bind(&config.created_at)
        .bind(&config.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(config)
    }

    pub(in crate::store) async fn delete_external_mcp_config(
        &self,
        id: &str,
    ) -> Result<bool, String> {
        let result = sqlx::query("DELETE FROM external_mcp_configs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }
}
