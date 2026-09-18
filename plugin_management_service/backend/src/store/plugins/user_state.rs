// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppStore {
    pub async fn list_plugin_installations(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<Vec<PluginInstallationRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_installations WHERE owner_user_id=$1 AND device_id=$2 ORDER BY active DESC,last_checked_at DESC").bind(owner_user_id).bind(device_id).fetch_all(&self.pool).await.map_err(db_error)?)
    }
    pub async fn get_plugin_installation(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
    ) -> Result<Option<PluginInstallationRecord>, String> {
        decode_optional(sqlx::query_scalar("SELECT data FROM plugin_installations WHERE owner_user_id=$1 AND device_id=$2 AND plugin_id=$3").bind(owner_user_id).bind(device_id).bind(plugin_id).fetch_optional(&self.pool).await.map_err(db_error)?)
    }
    pub async fn get_preferred_plugin_installation(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
    ) -> Result<Option<PluginInstallationRecord>, String> {
        decode_optional(sqlx::query_scalar("SELECT data FROM plugin_installations WHERE owner_user_id=$1 AND plugin_id=$2 AND active AND data->>'install_status'='installed' AND data->>'availability_status'=ANY($3) AND data->>'dependency_status'='satisfied' AND data->>'permission_status'='satisfied' AND data->>'auth_status'='satisfied' ORDER BY last_checked_at DESC,(data->>'installed_at')::timestamptz DESC,id LIMIT 1").bind(owner_user_id).bind(plugin_id).bind(vec!["ready","partially_available"]).fetch_optional(&self.pool).await.map_err(db_error)?)
    }
    pub async fn replace_plugin_installation(
        &self,
        record: &PluginInstallationRecord,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_installations(id,owner_user_id,device_id,plugin_id,release_id,active,last_checked_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO UPDATE SET owner_user_id=EXCLUDED.owner_user_id,device_id=EXCLUDED.device_id,plugin_id=EXCLUDED.plugin_id,release_id=EXCLUDED.release_id,active=EXCLUDED.active,last_checked_at=EXCLUDED.last_checked_at,data=EXCLUDED.data").bind(&record.id).bind(&record.owner_user_id).bind(&record.device_id).bind(&record.plugin_id).bind(&record.release_id).bind(record.active).bind(timestamp(&record.last_checked_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
    pub async fn get_user_plugin_preference(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
    ) -> Result<Option<UserPluginPreferenceRecord>, String> {
        decode_optional(
            sqlx::query_scalar(
                "SELECT data FROM plugin_user_preferences WHERE owner_user_id=$1 AND plugin_id=$2",
            )
            .bind(owner_user_id)
            .bind(plugin_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }
    pub async fn replace_user_plugin_preference(
        &self,
        record: &UserPluginPreferenceRecord,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_user_preferences(owner_user_id,plugin_id,enabled,updated_at,data) VALUES($1,$2,$3,$4,$5) ON CONFLICT(owner_user_id,plugin_id) DO UPDATE SET enabled=EXCLUDED.enabled,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data").bind(&record.owner_user_id).bind(&record.plugin_id).bind(record.enabled).bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
    pub async fn list_enabled_user_plugin_preferences(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<UserPluginPreferenceRecord>, String> {
        decode_all(
            sqlx::query_scalar(
                "SELECT data FROM plugin_user_preferences WHERE owner_user_id=$1 AND enabled",
            )
            .bind(owner_user_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }
    pub async fn list_plugin_oauth_connections(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
    ) -> Result<Vec<PluginOAuthConnectionRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_oauth_connections WHERE owner_user_id=$1 AND device_id=$2 AND plugin_id=$3 ORDER BY provider,component_key").bind(owner_user_id).bind(device_id).bind(plugin_id).fetch_all(&self.pool).await.map_err(db_error)?)
    }
    pub async fn get_plugin_oauth_connection(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
        component_key: &str,
        provider: &str,
    ) -> Result<Option<PluginOAuthConnectionRecord>, String> {
        decode_optional(sqlx::query_scalar("SELECT data FROM plugin_oauth_connections WHERE owner_user_id=$1 AND device_id=$2 AND plugin_id=$3 AND component_key=$4 AND provider=$5").bind(owner_user_id).bind(device_id).bind(plugin_id).bind(component_key).bind(provider).fetch_optional(&self.pool).await.map_err(db_error)?)
    }
    pub async fn replace_plugin_oauth_connection(
        &self,
        record: &PluginOAuthConnectionRecord,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_oauth_connections(id,owner_user_id,device_id,plugin_id,release_id,component_key,provider,connected,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(id) DO UPDATE SET owner_user_id=EXCLUDED.owner_user_id,device_id=EXCLUDED.device_id,plugin_id=EXCLUDED.plugin_id,release_id=EXCLUDED.release_id,component_key=EXCLUDED.component_key,provider=EXCLUDED.provider,connected=EXCLUDED.connected,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data").bind(&record.id).bind(&record.owner_user_id).bind(&record.device_id).bind(&record.plugin_id).bind(&record.release_id).bind(&record.component_key).bind(&record.provider).bind(record.connected).bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
    pub async fn insert_plugin_audit(&self, record: &PluginAuditLogRecord) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_audit_logs(id,event,owner_user_id,device_id,plugin_id,release_id,component_key,outcome,created_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&record.id).bind(&record.event).bind(&record.owner_user_id).bind(&record.device_id).bind(&record.plugin_id).bind(&record.release_id).bind(&record.component_key).bind(&record.outcome).bind(timestamp(&record.created_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
    pub async fn list_plugin_audit(
        &self,
        query: &PluginAuditQuery,
    ) -> Result<ListResponse<PluginAuditLogRecord>, String> {
        let plugin_id = normalized(query.plugin_id.as_deref());
        let owner = normalized(query.owner_user_id.as_deref());
        let device = normalized(query.device_id.as_deref());
        let event = normalized(query.event.as_deref());
        let (before_created_at, before_id) = match query.cursor()? {
            Some((created_at, id)) => (Some(timestamp(created_at)?), Some(id)),
            None => (None, None),
        };
        let total=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM plugin_audit_logs WHERE ($1::text IS NULL OR plugin_id=$1) AND ($2::text IS NULL OR owner_user_id=$2) AND ($3::text IS NULL OR device_id=$3) AND ($4::text IS NULL OR event=$4)").bind(&plugin_id).bind(&owner).bind(&device).bind(&event).fetch_one(&self.pool).await.map_err(db_error)?;
        let items=decode_all(sqlx::query_scalar("SELECT data FROM plugin_audit_logs WHERE ($1::text IS NULL OR plugin_id=$1) AND ($2::text IS NULL OR owner_user_id=$2) AND ($3::text IS NULL OR device_id=$3) AND ($4::text IS NULL OR event=$4) AND ($5::timestamptz IS NULL OR (created_at,id)<($5,$6::text)) ORDER BY created_at DESC,id DESC LIMIT $7 OFFSET $8").bind(plugin_id).bind(owner).bind(device).bind(event).bind(before_created_at).bind(before_id).bind(query.limit.unwrap_or(100).clamp(1,500)).bind(i64::try_from(query.offset.unwrap_or(0)).unwrap_or(i64::MAX)).fetch_all(&self.pool).await.map_err(db_error)?)?;
        Ok(ListResponse {
            items,
            total: u64::try_from(total).unwrap_or(u64::MAX),
        })
    }
}

#[cfg(test)]
mod tests;
