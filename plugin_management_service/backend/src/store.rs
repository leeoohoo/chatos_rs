// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use sqlx::{migrate::Migrator, types::Json};

use crate::models::*;

mod agents;
mod plugins;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

#[derive(Clone)]
pub struct AppStore {
    pub(super) pool: chatos_postgres::PgPool,
}

impl AppStore {
    pub fn new(pool: chatos_postgres::PgPool) -> Self {
        Self { pool }
    }

    pub(crate) fn pool(&self) -> &chatos_postgres::PgPool {
        &self.pool
    }

    pub async fn initialize(&self) -> Result<(), String> {
        chatos_postgres::ensure_migrations_applied(&self.pool, &MIGRATOR)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_mcps(
        &self,
        user: &CurrentUser,
        query: &ListResourcesQuery,
    ) -> Result<ListResponse<McpRecord>, String> {
        let (
            is_admin,
            owner,
            owner_filter,
            include_system,
            visibility,
            enabled,
            runtime,
            search,
            limit,
            offset,
        ) = list_parameters(user, query);
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM plugin_mcps WHERE
             ($1 OR ((owner_user_id=$2 AND visibility=$3) OR visibility=$4))
             AND (NOT $1 OR $5::text IS NULL OR owner_user_id=$5)
             AND ($1 OR TRUE) AND ($6 OR visibility<>$7)
             AND ($8::text IS NULL OR visibility=$8) AND ($9::bool IS NULL OR enabled=$9)
             AND ($10::text IS NULL OR runtime_kind=$10)
             AND ($11::text IS NULL OR name ILIKE $11 OR display_name ILIKE $11 OR data->>'description' ILIKE $11)"
        ).bind(is_admin).bind(&owner).bind(VISIBILITY_PRIVATE).bind(VISIBILITY_PUBLIC)
            .bind(&owner_filter).bind(include_system).bind(VISIBILITY_SYSTEM_PRIVATE).bind(&visibility)
            .bind(enabled).bind(&runtime).bind(search.clone())
            .fetch_one(&self.pool).await.map_err(db_error)?;
        let items = decode_all(sqlx::query_scalar(
            "SELECT data FROM plugin_mcps WHERE
             ($1 OR ((owner_user_id=$2 AND visibility=$3) OR visibility=$4))
             AND (NOT $1 OR $5::text IS NULL OR owner_user_id=$5) AND ($6 OR visibility<>$7)
             AND ($8::text IS NULL OR visibility=$8) AND ($9::bool IS NULL OR enabled=$9)
             AND ($10::text IS NULL OR runtime_kind=$10)
             AND ($11::text IS NULL OR name ILIKE $11 OR display_name ILIKE $11 OR data->>'description' ILIKE $11)
             ORDER BY updated_at DESC,(data->>'created_at')::timestamptz DESC LIMIT $12 OFFSET $13"
        ).bind(is_admin).bind(owner).bind(VISIBILITY_PRIVATE).bind(VISIBILITY_PUBLIC)
            .bind(owner_filter).bind(include_system).bind(VISIBILITY_SYSTEM_PRIVATE).bind(visibility)
            .bind(enabled).bind(runtime).bind(search).bind(limit).bind(offset)
            .fetch_all(&self.pool).await.map_err(db_error)?)?;
        Ok(ListResponse {
            items,
            total: u64::try_from(total).unwrap_or(u64::MAX),
        })
    }

    pub async fn get_mcp(&self, id: &str) -> Result<Option<McpRecord>, String> {
        fetch_one("SELECT data FROM plugin_mcps WHERE id=$1", id, &self.pool).await
    }

    pub async fn list_system_mcps(&self) -> Result<Vec<McpRecord>, String> {
        decode_all(
            sqlx::query_scalar(
                "SELECT data FROM plugin_mcps WHERE visibility=$1 ORDER BY display_name,name",
            )
            .bind(VISIBILITY_SYSTEM_PRIVATE)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn list_all_mcps_for_admin_catalog(&self) -> Result<Vec<McpRecord>, String> {
        decode_all(
            sqlx::query_scalar(
                "SELECT data FROM plugin_mcps ORDER BY visibility,display_name,name",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn remove_retired_direct_local_mcps(&self) -> Result<u64, String> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM plugin_mcps WHERE source_kind='local_connector_discovered'",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        self.delete_mcp_resources(&ids).await
    }

    async fn delete_mcp_resources(&self, ids: &[String]) -> Result<u64, String> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        sqlx::query(
            "DELETE FROM plugin_agent_bindings WHERE resource_kind=$1 AND resource_id=ANY($2)",
        )
        .bind(RESOURCE_KIND_MCP)
        .bind(ids)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        sqlx::query(
            "DELETE FROM plugin_resource_checks WHERE resource_kind=$1 AND resource_id=ANY($2)",
        )
        .bind(RESOURCE_KIND_MCP)
        .bind(ids)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        let deleted = sqlx::query("DELETE FROM plugin_mcps WHERE id=ANY($1)")
            .bind(ids)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?
            .rows_affected();
        tx.commit().await.map_err(db_error)?;
        Ok(deleted)
    }

    pub async fn list_enabled_user_mcps(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<McpRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_mcps WHERE enabled AND ((owner_user_id=$1 AND source_kind=$2 AND visibility=$3) OR visibility=$4)")
            .bind(owner_user_id).bind(SOURCE_KIND_USER_CREATED).bind(VISIBILITY_PRIVATE).bind(VISIBILITY_PUBLIC)
            .fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn replace_mcp(&self, record: &McpRecord) -> Result<(), String> {
        let (plugin_id, release_id, component_key) = component_columns(&record.plugin_component);
        sqlx::query("INSERT INTO plugin_mcps(id,owner_user_id,visibility,source_kind,name,display_name,enabled,runtime_kind,plugin_id,release_id,component_key,updated_at,data)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(id) DO UPDATE SET owner_user_id=EXCLUDED.owner_user_id,visibility=EXCLUDED.visibility,source_kind=EXCLUDED.source_kind,name=EXCLUDED.name,display_name=EXCLUDED.display_name,enabled=EXCLUDED.enabled,runtime_kind=EXCLUDED.runtime_kind,plugin_id=EXCLUDED.plugin_id,release_id=EXCLUDED.release_id,component_key=EXCLUDED.component_key,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data")
            .bind(&record.id).bind(&record.owner_user_id).bind(&record.visibility).bind(&record.source_kind)
            .bind(&record.name).bind(&record.display_name).bind(record.enabled).bind(&record.runtime.kind)
            .bind(plugin_id).bind(release_id).bind(component_key).bind(timestamp(&record.updated_at)?).bind(json(record)?)
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn delete_mcp(&self, id: &str) -> Result<(), String> {
        self.delete_mcp_resources(&[id.to_string()])
            .await
            .map(|_| ())
    }

    pub async fn remove_system_seed_mcps_except(
        &self,
        active_resource_ids: &[String],
    ) -> Result<u64, String> {
        if active_resource_ids.is_empty() {
            return Err("refusing to reconcile system MCP seeds with an empty catalog".to_string());
        }
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM plugin_mcps WHERE source_kind=$1 AND NOT(id=ANY($2))",
        )
        .bind(SOURCE_KIND_SYSTEM_SEED)
        .bind(active_resource_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        self.delete_mcp_resources(&ids).await
    }

    pub async fn list_skills(
        &self,
        user: &CurrentUser,
        query: &ListResourcesQuery,
    ) -> Result<ListResponse<SkillRecord>, String> {
        let (
            is_admin,
            owner,
            owner_filter,
            include_system,
            visibility,
            enabled,
            runtime,
            search,
            limit,
            offset,
        ) = list_parameters(user, query);
        let total=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM plugin_skills WHERE ($1 OR ((owner_user_id=$2 AND visibility=$3) OR visibility=$4)) AND (NOT $1 OR $5::text IS NULL OR owner_user_id=$5) AND ($6 OR visibility<>$7) AND ($8::text IS NULL OR visibility=$8) AND ($9::bool IS NULL OR enabled=$9) AND ($10::text IS NULL OR content_kind=$10) AND ($11::text IS NULL OR name ILIKE $11 OR display_name ILIKE $11 OR data->>'description' ILIKE $11)")
            .bind(is_admin).bind(&owner).bind(VISIBILITY_PRIVATE).bind(VISIBILITY_PUBLIC).bind(&owner_filter).bind(include_system).bind(VISIBILITY_SYSTEM_PRIVATE).bind(&visibility).bind(enabled).bind(&runtime).bind(&search)
            .fetch_one(&self.pool).await.map_err(db_error)?;
        let items=decode_all(sqlx::query_scalar("SELECT data FROM plugin_skills WHERE ($1 OR ((owner_user_id=$2 AND visibility=$3) OR visibility=$4)) AND (NOT $1 OR $5::text IS NULL OR owner_user_id=$5) AND ($6 OR visibility<>$7) AND ($8::text IS NULL OR visibility=$8) AND ($9::bool IS NULL OR enabled=$9) AND ($10::text IS NULL OR content_kind=$10) AND ($11::text IS NULL OR name ILIKE $11 OR display_name ILIKE $11 OR data->>'description' ILIKE $11) ORDER BY updated_at DESC,(data->>'created_at')::timestamptz DESC LIMIT $12 OFFSET $13")
            .bind(is_admin).bind(owner).bind(VISIBILITY_PRIVATE).bind(VISIBILITY_PUBLIC).bind(owner_filter).bind(include_system).bind(VISIBILITY_SYSTEM_PRIVATE).bind(visibility).bind(enabled).bind(runtime).bind(search).bind(limit).bind(offset)
            .fetch_all(&self.pool).await.map_err(db_error)?)?;
        Ok(ListResponse {
            items,
            total: u64::try_from(total).unwrap_or(u64::MAX),
        })
    }

    pub async fn get_skill(&self, id: &str) -> Result<Option<SkillRecord>, String> {
        fetch_one("SELECT data FROM plugin_skills WHERE id=$1", id, &self.pool).await
    }

    pub async fn remove_retired_builtin_skills(&self) -> Result<u64, String> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM plugin_skills WHERE visibility=$1 AND id LIKE 'internal_skill_%'",
        )
        .bind(VISIBILITY_SYSTEM_PRIVATE)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        if ids.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        sqlx::query(
            "DELETE FROM plugin_agent_bindings WHERE resource_kind=$1 AND resource_id=ANY($2)",
        )
        .bind(RESOURCE_KIND_SKILL)
        .bind(&ids)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        sqlx::query(
            "DELETE FROM plugin_resource_checks WHERE resource_kind=$1 AND resource_id=ANY($2)",
        )
        .bind(RESOURCE_KIND_SKILL)
        .bind(&ids)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        let count = sqlx::query("DELETE FROM plugin_skills WHERE id=ANY($1)")
            .bind(&ids)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?
            .rows_affected();
        tx.commit().await.map_err(db_error)?;
        Ok(count)
    }

    pub async fn list_enabled_user_skills(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<SkillRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_skills WHERE enabled AND ((owner_user_id=$1 AND source_kind=$2 AND visibility=$3) OR visibility=$4)").bind(owner_user_id).bind(SOURCE_KIND_USER_CREATED).bind(VISIBILITY_PRIVATE).bind(VISIBILITY_PUBLIC).fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn replace_skill(&self, record: &SkillRecord) -> Result<(), String> {
        let (plugin_id, release_id, component_key) = component_columns(&record.plugin_component);
        sqlx::query("INSERT INTO plugin_skills(id,owner_user_id,visibility,source_kind,name,display_name,enabled,content_kind,plugin_id,release_id,component_key,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(id) DO UPDATE SET owner_user_id=EXCLUDED.owner_user_id,visibility=EXCLUDED.visibility,source_kind=EXCLUDED.source_kind,name=EXCLUDED.name,display_name=EXCLUDED.display_name,enabled=EXCLUDED.enabled,content_kind=EXCLUDED.content_kind,plugin_id=EXCLUDED.plugin_id,release_id=EXCLUDED.release_id,component_key=EXCLUDED.component_key,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data")
            .bind(&record.id).bind(&record.owner_user_id).bind(&record.visibility).bind(&record.source_kind).bind(&record.name).bind(&record.display_name).bind(record.enabled).bind(&record.content.kind).bind(plugin_id).bind(release_id).bind(component_key).bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }

    pub async fn list_skill_packages(
        &self,
        user: &CurrentUser,
        query: &ListResourcesQuery,
    ) -> Result<ListResponse<SkillPackageRecord>, String> {
        let (
            is_admin,
            owner,
            owner_filter,
            include_system,
            visibility,
            _enabled,
            _runtime,
            search,
            limit,
            offset,
        ) = list_parameters(user, query);
        let total=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM plugin_skill_packages WHERE ($1 OR ((owner_user_id=$2 AND visibility=$3) OR visibility=$4)) AND (NOT $1 OR $5::text IS NULL OR owner_user_id=$5) AND ($6 OR visibility<>$7) AND ($8::text IS NULL OR visibility=$8) AND ($9::text IS NULL OR name ILIKE $9 OR data->>'description' ILIKE $9)").bind(is_admin).bind(&owner).bind(VISIBILITY_PRIVATE).bind(VISIBILITY_PUBLIC).bind(&owner_filter).bind(include_system).bind(VISIBILITY_SYSTEM_PRIVATE).bind(&visibility).bind(&search).fetch_one(&self.pool).await.map_err(db_error)?;
        let items=decode_all(sqlx::query_scalar("SELECT data FROM plugin_skill_packages WHERE ($1 OR ((owner_user_id=$2 AND visibility=$3) OR visibility=$4)) AND (NOT $1 OR $5::text IS NULL OR owner_user_id=$5) AND ($6 OR visibility<>$7) AND ($8::text IS NULL OR visibility=$8) AND ($9::text IS NULL OR name ILIKE $9 OR data->>'description' ILIKE $9) ORDER BY updated_at DESC,(data->>'created_at')::timestamptz DESC LIMIT $10 OFFSET $11").bind(is_admin).bind(owner).bind(VISIBILITY_PRIVATE).bind(VISIBILITY_PUBLIC).bind(owner_filter).bind(include_system).bind(VISIBILITY_SYSTEM_PRIVATE).bind(visibility).bind(search).bind(limit).bind(offset).fetch_all(&self.pool).await.map_err(db_error)?)?;
        Ok(ListResponse {
            items,
            total: u64::try_from(total).unwrap_or(u64::MAX),
        })
    }

    pub async fn get_skill_package(&self, id: &str) -> Result<Option<SkillPackageRecord>, String> {
        fetch_one(
            "SELECT data FROM plugin_skill_packages WHERE id=$1",
            id,
            &self.pool,
        )
        .await
    }
    pub async fn get_check(
        &self,
        resource_kind: &str,
        resource_id: &str,
    ) -> Result<Option<ResourceCheckRecord>, String> {
        decode_optional(sqlx::query_scalar("SELECT data FROM plugin_resource_checks WHERE resource_kind=$1 AND resource_id=$2 ORDER BY last_checked_at DESC LIMIT 1").bind(resource_kind).bind(resource_id).fetch_optional(&self.pool).await.map_err(db_error)?)
    }
    pub async fn replace_check(&self, record: &ResourceCheckRecord) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_resource_checks(id,resource_kind,resource_id,owner_user_id,status,last_checked_at,data) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO UPDATE SET resource_kind=EXCLUDED.resource_kind,resource_id=EXCLUDED.resource_id,owner_user_id=EXCLUDED.owner_user_id,status=EXCLUDED.status,last_checked_at=EXCLUDED.last_checked_at,data=EXCLUDED.data").bind(&record.id).bind(&record.resource_kind).bind(&record.resource_id).bind(&record.owner_user_id).bind(&record.status).bind(timestamp(&record.last_checked_at)?).bind(json(record)?).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
}

type ListParameters = (
    bool,
    String,
    Option<String>,
    bool,
    Option<String>,
    Option<bool>,
    Option<String>,
    Option<String>,
    i64,
    i64,
);

fn list_parameters(user: &CurrentUser, query: &ListResourcesQuery) -> ListParameters {
    let search = normalized(query.q.as_deref()).map(|value| format!("%{value}%"));
    (
        user.is_super_admin(),
        user.effective_owner_user_id().to_string(),
        normalized(query.owner_user_id.as_deref()),
        query.include_system.unwrap_or(false),
        normalized(query.visibility.as_deref()),
        query.enabled,
        normalized(query.runtime_kind.as_deref()),
        search,
        query.limit.unwrap_or(100).clamp(1, 500),
        i64::try_from(query.offset.unwrap_or(0)).unwrap_or(i64::MAX),
    )
}

fn component_columns(
    ownership: &PluginComponentOwnership,
) -> (Option<&str>, Option<&str>, Option<&str>) {
    (
        ownership.plugin_id.as_deref(),
        ownership.release_id.as_deref(),
        ownership.component_key.as_deref(),
    )
}

pub(super) fn timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|err| format!("invalid RFC3339 timestamp {value:?}: {err}"))
}
pub(super) fn optional_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    value.map(timestamp).transpose()
}
pub(super) fn json<T: Serialize>(value: &T) -> Result<Json<Value>, String> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|err| err.to_string())
}
pub(super) fn decode_optional<T: DeserializeOwned>(
    value: Option<Json<Value>>,
) -> Result<Option<T>, String> {
    value
        .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
        .transpose()
}
pub(super) fn decode_all<T: DeserializeOwned>(values: Vec<Json<Value>>) -> Result<Vec<T>, String> {
    values
        .into_iter()
        .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
        .collect()
}
const POSTGRES_UNIQUE_VIOLATION_MARKER: &str = "postgres_unique_violation: ";

pub(crate) fn is_unique_violation(error: &str) -> bool {
    error.starts_with(POSTGRES_UNIQUE_VIOLATION_MARKER)
}

pub(super) fn db_error(error: sqlx::Error) -> String {
    match &error {
        sqlx::Error::Database(database_error) if database_error.is_unique_violation() => {
            format!("{POSTGRES_UNIQUE_VIOLATION_MARKER}{error}")
        }
        _ => error.to_string(),
    }
}

pub(super) async fn fetch_one<T: DeserializeOwned>(
    query: &str,
    id: &str,
    pool: &chatos_postgres::PgPool,
) -> Result<Option<T>, String> {
    decode_optional(
        sqlx::query_scalar(query)
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?,
    )
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}
pub fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod postgres_error_tests {
    use super::*;

    #[test]
    fn unique_violation_detection_does_not_accept_legacy_database_errors() {
        assert!(is_unique_violation(
            "postgres_unique_violation: duplicate key"
        ));
        assert!(!is_unique_violation("unclassified duplicate key error"));
    }
}
