// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_plugin_management_sdk::AgentPromptVendor;

impl AppStore {
    pub async fn list_agents(&self) -> Result<Vec<SystemAgentRecord>, String> {
        decode_all(
            sqlx::query_scalar(
                "SELECT data FROM plugin_agents ORDER BY data->>'display_name',agent_key",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }
    pub async fn get_agent(&self, agent_key: &str) -> Result<Option<SystemAgentRecord>, String> {
        fetch_one(
            "SELECT data FROM plugin_agents WHERE agent_key=$1",
            agent_key,
            &self.pool,
        )
        .await
    }
    pub async fn replace_agent(&self, record: &SystemAgentRecord) -> Result<(), String> {
        let (plugin_id, release_id, component_key) = component_columns(&record.plugin_component);
        sqlx::query("INSERT INTO plugin_agents(agent_key,service_name,enabled,plugin_id,release_id,component_key,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(agent_key) DO UPDATE SET service_name=EXCLUDED.service_name,enabled=EXCLUDED.enabled,plugin_id=EXCLUDED.plugin_id,release_id=EXCLUDED.release_id,component_key=EXCLUDED.component_key,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data")
            .bind(&record.agent_key).bind(&record.service_name).bind(record.enabled).bind(plugin_id).bind(release_id).bind(component_key).bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
    pub async fn delete_agent(&self, agent_key: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM plugin_agents WHERE agent_key=$1")
            .bind(agent_key)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error)
    }
    pub async fn delete_retired_agent_state(&self, agent_key: &str) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        for query in [
            "DELETE FROM plugin_agent_provider_prompts WHERE agent_key=$1",
            "DELETE FROM plugin_agent_prompt_releases WHERE agent_key=$1",
            "DELETE FROM plugin_agent_bindings WHERE agent_key=$1",
            "DELETE FROM plugin_agents WHERE agent_key=$1",
        ] {
            sqlx::query(query)
                .bind(agent_key)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
        }
        tx.commit().await.map_err(db_error)
    }

    pub async fn list_agent_prompts(
        &self,
        agent_key: &str,
    ) -> Result<Vec<AgentProviderPromptRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_agent_provider_prompts WHERE agent_key=$1 ORDER BY profile,vendor").bind(agent_key).fetch_all(&self.pool).await.map_err(db_error)?)
    }
    pub async fn get_agent_prompt(
        &self,
        agent_key: &str,
        profile: &str,
        vendor: AgentPromptVendor,
    ) -> Result<Option<AgentProviderPromptRecord>, String> {
        let vendor = enum_text(&vendor)?;
        decode_optional(sqlx::query_scalar("SELECT data FROM plugin_agent_provider_prompts WHERE agent_key=$1 AND profile=$2 AND vendor=$3").bind(agent_key).bind(profile).bind(vendor).fetch_optional(&self.pool).await.map_err(db_error)?)
    }
    pub async fn list_published_agent_prompts(
        &self,
    ) -> Result<Vec<AgentProviderPromptRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_agent_provider_prompts WHERE enabled AND data->>'published_content' IS NOT NULL ORDER BY agent_key,profile,vendor").fetch_all(&self.pool).await.map_err(db_error)?)
    }
    pub async fn replace_agent_prompt(
        &self,
        record: &AgentProviderPromptRecord,
    ) -> Result<(), String> {
        let vendor = enum_text(&record.vendor)?;
        sqlx::query("INSERT INTO plugin_agent_provider_prompts(id,agent_key,profile,vendor,enabled,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO UPDATE SET agent_key=EXCLUDED.agent_key,profile=EXCLUDED.profile,vendor=EXCLUDED.vendor,enabled=EXCLUDED.enabled,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data").bind(&record.id).bind(&record.agent_key).bind(&record.profile).bind(vendor).bind(record.enabled).bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
    pub async fn remove_agent_prompt_profiles_except(
        &self,
        agent_key: &str,
        active_profiles: &[&str],
    ) -> Result<bool, String> {
        if active_profiles.is_empty() {
            return Err(
                "refusing to reconcile Agent prompt profiles with an empty set".to_string(),
            );
        }
        sqlx::query(
            "DELETE FROM plugin_agent_provider_prompts WHERE agent_key=$1 AND NOT(profile=ANY($2))",
        )
        .bind(agent_key)
        .bind(active_profiles)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() > 0)
        .map_err(db_error)
    }

    pub async fn get_agent_prompt_bundle_version(
        &self,
    ) -> Result<Option<AgentPromptBundleVersionRecord>, String> {
        fetch_one(
            "SELECT data FROM plugin_agent_prompt_versions WHERE id=$1",
            "system_agent_prompts",
            &self.pool,
        )
        .await
    }
    pub async fn replace_agent_prompt_bundle_version(
        &self,
        record: &AgentPromptBundleVersionRecord,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_agent_prompt_versions(id,version,updated_at,data) VALUES($1,$2,$3,$4) ON CONFLICT(id) DO UPDATE SET version=EXCLUDED.version,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data").bind(&record.id).bind(record.version).bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
    pub async fn increment_agent_prompt_bundle_version(
        &self,
    ) -> Result<AgentPromptBundleVersionRecord, String> {
        let updated_at = now_rfc3339();
        let value = sqlx::query_scalar::<_, Json<Value>>("UPDATE plugin_agent_prompt_versions SET version=version+1,updated_at=$1,data=jsonb_set(jsonb_set(data,'{version}',to_jsonb(version+1)),'{updated_at}',to_jsonb($2::text)) WHERE id='system_agent_prompts' RETURNING data")
            .bind(timestamp(&updated_at)?).bind(&updated_at).fetch_optional(&self.pool).await.map_err(db_error)?
            .ok_or_else(|| "Agent Prompt bundle version is not initialized".to_string())?;
        serde_json::from_value(value.0).map_err(|error| error.to_string())
    }
    pub async fn list_agent_prompt_versions(
        &self,
        agent_key: &str,
    ) -> Result<Vec<AgentPromptVersionRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_agent_prompt_releases WHERE agent_key=$1 ORDER BY bundle_version DESC LIMIT 500").bind(agent_key).fetch_all(&self.pool).await.map_err(db_error)?)
    }
    pub async fn get_agent_prompt_version(
        &self,
        agent_key: &str,
        bundle_version: i64,
    ) -> Result<Option<AgentPromptVersionRecord>, String> {
        decode_optional(sqlx::query_scalar("SELECT data FROM plugin_agent_prompt_releases WHERE agent_key=$1 AND bundle_version=$2").bind(agent_key).bind(bundle_version).fetch_optional(&self.pool).await.map_err(db_error)?)
    }
    pub async fn replace_agent_prompt_version(
        &self,
        record: &AgentPromptVersionRecord,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_agent_prompt_releases(id,agent_key,bundle_version,published_at,data) VALUES($1,$2,$3,$4,$5) ON CONFLICT(id) DO UPDATE SET agent_key=EXCLUDED.agent_key,bundle_version=EXCLUDED.bundle_version,published_at=EXCLUDED.published_at,data=EXCLUDED.data").bind(&record.id).bind(&record.agent_key).bind(record.bundle_version).bind(timestamp(&record.published_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }

    pub async fn list_bindings(
        &self,
        agent_key: &str,
        query: &ListBindingsQuery,
    ) -> Result<Vec<AgentBindingRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_agent_bindings WHERE agent_key=$1 AND ($2::text IS NULL OR binding_scope=$2) AND ($3::text IS NULL OR owner_user_id=$3) ORDER BY priority,(data->>'created_at')::timestamptz").bind(agent_key).bind(normalized(query.scope.as_deref())).bind(normalized(query.owner_user_id.as_deref())).fetch_all(&self.pool).await.map_err(db_error)?)
    }
    pub async fn list_bindings_for_runtime(
        &self,
        agent_key: &str,
        owner_user_id: &str,
    ) -> Result<Vec<AgentBindingRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_agent_bindings WHERE agent_key=$1 AND enabled AND (owner_user_id IS NULL OR owner_user_id=$2) ORDER BY priority,(data->>'created_at')::timestamptz").bind(agent_key).bind(owner_user_id).fetch_all(&self.pool).await.map_err(db_error)?)
    }
    pub async fn replace_binding(&self, record: &AgentBindingRecord) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_agent_bindings(id,agent_key,binding_scope,owner_user_id,resource_kind,resource_id,enabled,priority,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(id) DO UPDATE SET agent_key=EXCLUDED.agent_key,binding_scope=EXCLUDED.binding_scope,owner_user_id=EXCLUDED.owner_user_id,resource_kind=EXCLUDED.resource_kind,resource_id=EXCLUDED.resource_id,enabled=EXCLUDED.enabled,priority=EXCLUDED.priority,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data").bind(&record.id).bind(&record.agent_key).bind(&record.binding_scope).bind(&record.owner_user_id).bind(&record.resource_kind).bind(&record.resource_id).bind(record.enabled).bind(record.priority).bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
    pub async fn get_binding(&self, id: &str) -> Result<Option<AgentBindingRecord>, String> {
        fetch_one(
            "SELECT data FROM plugin_agent_bindings WHERE id=$1",
            id,
            &self.pool,
        )
        .await
    }
    pub async fn delete_binding(&self, id: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM plugin_agent_bindings WHERE id=$1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error)
    }
    pub async fn delete_mcp_bindings_for_agent(&self, agent_key: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM plugin_agent_bindings WHERE agent_key=$1 AND resource_kind=$2")
            .bind(agent_key)
            .bind(RESOURCE_KIND_MCP)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error)
    }
    pub async fn delete_bindings_for_agent(&self, agent_key: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM plugin_agent_bindings WHERE agent_key=$1")
            .bind(agent_key)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error)
    }
}

fn enum_text<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_value(value)
        .map_err(|err| err.to_string())?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "enum did not serialize as text".to_string())
}
