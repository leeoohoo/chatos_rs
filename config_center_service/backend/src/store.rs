// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use sqlx::migrate::Migrator;
use sqlx::types::Json;

use chatos_config_sdk::{ConfigSnapshot, PlatformPressureLevel};
use chatos_postgres::PgPool;

use crate::models::{
    ActiveReleaseRecord, AuditEventRecord, ConfigDefinitionRecord, ConfigDraftRecord,
    ConfigReleaseRecord, PlatformPressureStateRecord, ServiceInstanceRecord,
};

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

#[derive(Clone)]
pub struct AppStore {
    pool: PgPool,
}

impl AppStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn initialize(&self) -> Result<(), String> {
        chatos_postgres::ensure_migrations_applied(&self.pool, &MIGRATOR)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn ping(&self) -> Result<(), String> {
        chatos_postgres::check_health(&self.pool)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn upsert_definition(
        &self,
        definition: &ConfigDefinitionRecord,
    ) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO config_definitions (key, ui_order, created_at, updated_at, data)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (key) DO UPDATE SET
                ui_order = EXCLUDED.ui_order,
                updated_at = EXCLUDED.updated_at,
                data = EXCLUDED.data
            "#,
        )
        .bind(&definition.key)
        .bind(definition.ui_order)
        .bind(timestamp(&definition.created_at)?)
        .bind(timestamp(&definition.updated_at)?)
        .bind(json(definition)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn delete_definitions(&self, keys: &[&str]) -> Result<(), String> {
        sqlx::query("DELETE FROM config_definitions WHERE key = ANY($1)")
            .bind(keys)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error)
    }

    pub async fn list_definitions(&self) -> Result<Vec<ConfigDefinitionRecord>, String> {
        fetch_all(
            sqlx::query_scalar("SELECT data FROM config_definitions ORDER BY ui_order, key"),
            &self.pool,
        )
        .await
    }

    pub async fn get_active(
        &self,
        environment: &str,
    ) -> Result<Option<ActiveReleaseRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM config_active_releases WHERE environment = $1")
                .bind(environment),
            &self.pool,
        )
        .await
    }

    pub async fn list_active_releases(&self) -> Result<Vec<ActiveReleaseRecord>, String> {
        fetch_all(
            sqlx::query_scalar("SELECT data FROM config_active_releases ORDER BY environment"),
            &self.pool,
        )
        .await
    }

    pub async fn set_active(&self, active: &ActiveReleaseRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO config_active_releases
                (environment, release_id, revision, updated_at, data)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (environment) DO UPDATE SET
                release_id = EXCLUDED.release_id,
                revision = EXCLUDED.revision,
                updated_at = EXCLUDED.updated_at,
                data = EXCLUDED.data
            "#,
        )
        .bind(&active.environment)
        .bind(&active.release_id)
        .bind(active.revision)
        .bind(timestamp(&active.updated_at)?)
        .bind(json(active)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn get_release(&self, id: &str) -> Result<Option<ConfigReleaseRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM config_releases WHERE id = $1").bind(id),
            &self.pool,
        )
        .await
    }

    pub async fn get_active_release(
        &self,
        environment: &str,
    ) -> Result<Option<ConfigReleaseRecord>, String> {
        let Some(active) = self.get_active(environment).await? else {
            return Ok(None);
        };
        self.get_release(&active.release_id).await
    }

    pub async fn insert_release(&self, release: &ConfigReleaseRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO config_releases
                (id, environment, revision, status, created_at, published_at, data)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(&release.id)
        .bind(&release.environment)
        .bind(release.revision)
        .bind(&release.status)
        .bind(timestamp(&release.created_at)?)
        .bind(optional_timestamp(release.published_at.as_deref())?)
        .bind(json(release)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn save_release(&self, release: &ConfigReleaseRecord) -> Result<(), String> {
        let result = sqlx::query(
            r#"
            UPDATE config_releases SET
                environment = $2,
                revision = $3,
                status = $4,
                published_at = $5,
                data = $6
            WHERE id = $1
            "#,
        )
        .bind(&release.id)
        .bind(&release.environment)
        .bind(release.revision)
        .bind(&release.status)
        .bind(optional_timestamp(release.published_at.as_deref())?)
        .bind(json(release)?)
        .execute(&self.pool)
        .await
        .map_err(db_error)?;
        require_updated(result.rows_affected(), "config release")
    }

    pub async fn list_releases(
        &self,
        environment: &str,
        limit: i64,
    ) -> Result<Vec<ConfigReleaseRecord>, String> {
        fetch_all(
            sqlx::query_scalar(
                "SELECT data FROM config_releases WHERE environment = $1 ORDER BY revision DESC LIMIT $2",
            )
            .bind(environment)
            .bind(limit.max(1)),
            &self.pool,
        )
        .await
    }

    pub async fn list_all_releases(&self) -> Result<Vec<ConfigReleaseRecord>, String> {
        fetch_all(
            sqlx::query_scalar(
                "SELECT data FROM config_releases ORDER BY environment, revision DESC",
            ),
            &self.pool,
        )
        .await
    }

    pub async fn next_release_revision(&self, environment: &str) -> Result<i64, String> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM config_releases WHERE environment = $1",
        )
        .bind(environment)
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)
    }

    pub async fn insert_snapshot(&self, snapshot: &ConfigSnapshot) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO config_snapshots
                (environment, service_name, revision, checksum, generated_at, data)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&snapshot.environment)
        .bind(&snapshot.service_name)
        .bind(snapshot.revision)
        .bind(&snapshot.checksum)
        .bind(timestamp(&snapshot.generated_at)?)
        .bind(json(snapshot)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn list_all_snapshots(&self) -> Result<Vec<ConfigSnapshot>, String> {
        fetch_all(
            sqlx::query_scalar(
                "SELECT data FROM config_snapshots ORDER BY environment, service_name, revision",
            ),
            &self.pool,
        )
        .await
    }

    pub async fn save_snapshot(&self, snapshot: &ConfigSnapshot) -> Result<(), String> {
        let result = sqlx::query(
            r#"
            UPDATE config_snapshots SET checksum = $4, generated_at = $5, data = $6
            WHERE environment = $1 AND service_name = $2 AND revision = $3
            "#,
        )
        .bind(&snapshot.environment)
        .bind(&snapshot.service_name)
        .bind(snapshot.revision)
        .bind(&snapshot.checksum)
        .bind(timestamp(&snapshot.generated_at)?)
        .bind(json(snapshot)?)
        .execute(&self.pool)
        .await
        .map_err(db_error)?;
        require_updated(result.rows_affected(), "config snapshot")
    }

    pub async fn get_snapshot(
        &self,
        environment: &str,
        service_name: &str,
        revision: i64,
    ) -> Result<Option<ConfigSnapshot>, String> {
        fetch_optional(
            sqlx::query_scalar(
                "SELECT data FROM config_snapshots WHERE environment = $1 AND service_name = $2 AND revision = $3",
            )
            .bind(environment)
            .bind(service_name)
            .bind(revision),
            &self.pool,
        )
        .await
    }

    pub async fn get_active_snapshot(
        &self,
        environment: &str,
        service_name: &str,
    ) -> Result<Option<ConfigSnapshot>, String> {
        let Some(active) = self.get_active(environment).await? else {
            return Ok(None);
        };
        self.get_snapshot(environment, service_name, active.revision)
            .await
    }

    pub async fn get_draft(&self, environment: &str) -> Result<Option<ConfigDraftRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM config_drafts WHERE environment = $1")
                .bind(environment),
            &self.pool,
        )
        .await
    }

    pub async fn list_drafts(&self) -> Result<Vec<ConfigDraftRecord>, String> {
        fetch_all(
            sqlx::query_scalar("SELECT data FROM config_drafts ORDER BY environment"),
            &self.pool,
        )
        .await
    }

    pub async fn save_draft(&self, draft: &ConfigDraftRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO config_drafts (environment, base_revision, updated_at, data)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (environment) DO UPDATE SET
                base_revision = EXCLUDED.base_revision,
                updated_at = EXCLUDED.updated_at,
                data = EXCLUDED.data
            "#,
        )
        .bind(&draft.environment)
        .bind(draft.base_revision)
        .bind(timestamp(&draft.updated_at)?)
        .bind(json(draft)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn delete_draft(&self, environment: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM config_drafts WHERE environment = $1")
            .bind(environment)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error)
    }

    pub async fn insert_audit(&self, event: &AuditEventRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO config_audit_events
                (id, environment, action, actor_user_id, release_id, created_at, data)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(&event.id)
        .bind(&event.environment)
        .bind(&event.action)
        .bind(&event.actor_user_id)
        .bind(&event.release_id)
        .bind(timestamp(&event.created_at)?)
        .bind(json(event)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn list_audit(&self, limit: i64) -> Result<Vec<AuditEventRecord>, String> {
        fetch_all(
            sqlx::query_scalar(
                "SELECT data FROM config_audit_events ORDER BY created_at DESC, id LIMIT $1",
            )
            .bind(limit.max(1)),
            &self.pool,
        )
        .await
    }

    pub async fn list_all_audit(&self) -> Result<Vec<AuditEventRecord>, String> {
        fetch_all(
            sqlx::query_scalar("SELECT data FROM config_audit_events ORDER BY created_at DESC, id"),
            &self.pool,
        )
        .await
    }

    pub async fn save_audit(&self, event: &AuditEventRecord) -> Result<(), String> {
        let result = sqlx::query(
            r#"
            UPDATE config_audit_events SET
                environment = $2, action = $3, actor_user_id = $4,
                release_id = $5, created_at = $6, data = $7
            WHERE id = $1
            "#,
        )
        .bind(&event.id)
        .bind(&event.environment)
        .bind(&event.action)
        .bind(&event.actor_user_id)
        .bind(&event.release_id)
        .bind(timestamp(&event.created_at)?)
        .bind(json(event)?)
        .execute(&self.pool)
        .await
        .map_err(db_error)?;
        require_updated(result.rows_affected(), "audit event")
    }

    pub async fn upsert_instance(&self, instance: &ServiceInstanceRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO config_service_instances
                (id, environment, service_name, service_id, effective_revision, last_seen_at, data)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (environment, service_name, service_id) DO UPDATE SET
                id = EXCLUDED.id,
                effective_revision = EXCLUDED.effective_revision,
                last_seen_at = EXCLUDED.last_seen_at,
                data = EXCLUDED.data
            "#,
        )
        .bind(&instance.id)
        .bind(&instance.environment)
        .bind(&instance.service_name)
        .bind(&instance.service_id)
        .bind(instance.effective_revision)
        .bind(timestamp(&instance.last_seen_at)?)
        .bind(json(instance)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn list_instances(&self) -> Result<Vec<ServiceInstanceRecord>, String> {
        fetch_all(
            sqlx::query_scalar(
                "SELECT data FROM config_service_instances ORDER BY environment, service_name, service_id",
            ),
            &self.pool,
        )
        .await
    }

    pub async fn get_pressure_state(
        &self,
        environment: &str,
    ) -> Result<Option<PlatformPressureStateRecord>, String> {
        fetch_optional(
            sqlx::query_scalar(
                "SELECT data FROM config_platform_pressure_states WHERE environment = $1",
            )
            .bind(environment),
            &self.pool,
        )
        .await
    }

    pub async fn upsert_pressure_state(
        &self,
        state: &PlatformPressureStateRecord,
    ) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO config_platform_pressure_states (environment, level, updated_at, data)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (environment) DO UPDATE SET
                level = EXCLUDED.level,
                updated_at = EXCLUDED.updated_at,
                data = EXCLUDED.data
            "#,
        )
        .bind(&state.environment)
        .bind(state.level.as_str())
        .bind(timestamp(&state.updated_at)?)
        .bind(json(state)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn replace_pressure_state_if_level(
        &self,
        environment: &str,
        expected: PlatformPressureLevel,
        next: &PlatformPressureStateRecord,
    ) -> Result<bool, String> {
        sqlx::query(
            r#"
            UPDATE config_platform_pressure_states
            SET level = $3, updated_at = $4, data = $5
            WHERE environment = $1 AND level = $2
            "#,
        )
        .bind(environment)
        .bind(expected.as_str())
        .bind(next.level.as_str())
        .bind(timestamp(&next.updated_at)?)
        .bind(json(next)?)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(db_error)
    }
}

fn timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|err| format!("invalid RFC3339 timestamp {value:?}: {err}"))
}

fn optional_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    value.map(timestamp).transpose()
}

fn json<T: Serialize>(value: &T) -> Result<Json<Value>, String> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|err| err.to_string())
}

async fn fetch_all<'q, T>(
    query: sqlx::query::QueryScalar<'q, sqlx::Postgres, Json<Value>, sqlx::postgres::PgArguments>,
    pool: &PgPool,
) -> Result<Vec<T>, String>
where
    T: DeserializeOwned,
{
    query
        .fetch_all(pool)
        .await
        .map_err(db_error)?
        .into_iter()
        .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
        .collect()
}

async fn fetch_optional<'q, T>(
    query: sqlx::query::QueryScalar<'q, sqlx::Postgres, Json<Value>, sqlx::postgres::PgArguments>,
    pool: &PgPool,
) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    query
        .fetch_optional(pool)
        .await
        .map_err(db_error)?
        .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
        .transpose()
}

fn require_updated(rows_affected: u64, resource: &str) -> Result<(), String> {
    if rows_affected == 1 {
        Ok(())
    } else {
        Err(format!("{resource} not found"))
    }
}

fn db_error(error: sqlx::Error) -> String {
    error.to_string()
}
