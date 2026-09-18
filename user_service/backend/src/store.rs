// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use sqlx::migrate::Migrator;
use sqlx::types::Json;
use uuid::Uuid;

use crate::auth::{hash_password, normalize_display_name, normalize_username};
use crate::config::AppConfig;
use crate::models::{
    AgentAccountListItem, AgentAccountRecord, HarnessProvisioningRecord, InviteCodePublicRecord,
    InviteCodeRecord, LocalConnectorAuthTicketRecord, RegistrationEmailCodeRecord,
    UserOptionRecord, UserRecord, UserSummaryPageResponse, UserSummaryRecord,
    USER_ROLE_SUPER_ADMIN,
};
use chatos_service_runtime::is_production_environment;

mod model_configs;
pub(crate) mod wechat_auth;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

#[derive(Clone)]
pub struct AppStore {
    pub(crate) pool: chatos_postgres::PgPool,
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

    pub async fn ensure_default_super_admin(&self, config: &AppConfig) -> Result<(), String> {
        self.ensure_default_super_admin_for_environment(
            SuperAdminBootstrapConfig::from(config),
            is_production_environment(),
        )
        .await
    }

    async fn ensure_default_super_admin_for_environment(
        &self,
        config: SuperAdminBootstrapConfig<'_>,
        production: bool,
    ) -> Result<(), String> {
        let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        if count > 0 {
            let normalized = normalize_username(config.username)?;
            if let Some(mut user) = self.find_user_by_username(&normalized).await? {
                if user.role != USER_ROLE_SUPER_ADMIN {
                    user.role = USER_ROLE_SUPER_ADMIN.to_string();
                    user.updated_at = now_rfc3339();
                    self.update_user_record(&user).await?;
                }
            }
            return Ok(());
        }
        ensure_empty_database_bootstrap_allowed(
            production,
            config.allow_empty_database_admin_creation,
        )?;
        let username = normalize_username(config.username)?;
        let now = now_rfc3339();
        self.insert_user_record(&UserRecord {
            id: Uuid::new_v4().to_string(),
            username: username.clone(),
            display_name: normalize_display_name(Some(config.display_name), &username),
            password_hash: hash_password(config.password)?,
            role: USER_ROLE_SUPER_ADMIN.to_string(),
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
            last_login_at: None,
        })
        .await
    }

    pub async fn find_user_by_id(&self, id: &str) -> Result<Option<UserRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM users WHERE id = $1").bind(id),
            &self.pool,
        )
        .await
    }

    pub async fn find_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM users WHERE username = $1").bind(username),
            &self.pool,
        )
        .await
    }

    pub async fn list_users_summary(&self) -> Result<Vec<UserSummaryRecord>, String> {
        let users = fetch_all(
            sqlx::query_scalar(
                "SELECT data FROM users ORDER BY updated_at DESC, created_at DESC, id",
            ),
            &self.pool,
        )
        .await?;
        self.user_summaries_from_records(users).await
    }

    pub async fn list_user_options(
        &self,
        user_ids: Option<&[String]>,
    ) -> Result<Vec<UserOptionRecord>, String> {
        if user_ids.is_some_and(|ids| ids.is_empty()) {
            return Ok(Vec::new());
        }
        let users: Vec<UserRecord> = match user_ids {
            Some(ids) => {
                fetch_all(
                    sqlx::query_scalar(
                        "SELECT data FROM users WHERE id = ANY($1) ORDER BY username, id",
                    )
                    .bind(ids),
                    &self.pool,
                )
                .await?
            }
            None => {
                fetch_all(
                    sqlx::query_scalar("SELECT data FROM users ORDER BY username, id"),
                    &self.pool,
                )
                .await?
            }
        };
        Ok(users
            .into_iter()
            .map(|user| UserOptionRecord {
                id: user.id,
                username: user.username,
                display_name: user.display_name,
            })
            .collect())
    }

    pub async fn list_users_summary_page(
        &self,
        limit: i64,
        offset: u64,
    ) -> Result<UserSummaryPageResponse, String> {
        let (users, total) = tokio::try_join!(
            fetch_all(
                sqlx::query_scalar(
                    "SELECT data FROM users ORDER BY updated_at DESC, created_at DESC, id LIMIT $1 OFFSET $2",
                )
                .bind(limit.max(1))
                .bind(i64::try_from(offset).unwrap_or(i64::MAX)),
                &self.pool,
            ),
            async {
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_error)
            }
        )?;
        Ok(UserSummaryPageResponse {
            items: self.user_summaries_from_records(users).await?,
            total: u64::try_from(total).map_err(|err| err.to_string())?,
        })
    }

    async fn user_summaries_from_records(
        &self,
        users: Vec<UserRecord>,
    ) -> Result<Vec<UserSummaryRecord>, String> {
        if users.is_empty() {
            return Ok(Vec::new());
        }
        let user_ids = users.iter().map(|user| user.id.clone()).collect::<Vec<_>>();
        let (mut agent_counts, mut harness_by_user) = tokio::try_join!(
            self.agent_counts_for_user_ids(&user_ids),
            self.harness_by_user_ids(&user_ids),
        )?;
        Ok(users
            .into_iter()
            .map(|user| UserSummaryRecord {
                agent_count: agent_counts.remove(&user.id).unwrap_or(0),
                harness_provisioning: harness_by_user.remove(&user.id).map(Into::into),
                id: user.id,
                username: user.username,
                display_name: user.display_name,
                role: user.role,
                enabled: user.enabled,
                created_at: user.created_at,
                updated_at: user.updated_at,
                last_login_at: user.last_login_at,
            })
            .collect())
    }

    async fn agent_counts_for_user_ids(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, i64>, String> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT owner_user_id, count(*) FROM agent_accounts WHERE owner_user_id = ANY($1) GROUP BY owner_user_id",
        )
        .bind(user_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        Ok(rows.into_iter().collect())
    }

    async fn harness_by_user_ids(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, HarnessProvisioningRecord>, String> {
        let rows: Vec<HarnessProvisioningRecord> = fetch_all(
            sqlx::query_scalar("SELECT data FROM harness_provisioning WHERE user_id = ANY($1)")
                .bind(user_ids),
            &self.pool,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|record| (record.user_id.clone(), record))
            .collect())
    }

    pub async fn get_user_summary(&self, id: &str) -> Result<Option<UserSummaryRecord>, String> {
        let Some(user) = self.find_user_by_id(id).await? else {
            return Ok(None);
        };
        Ok(Some(self.user_summary_from_record(user).await?))
    }

    async fn user_summary_from_record(
        &self,
        user: UserRecord,
    ) -> Result<UserSummaryRecord, String> {
        let agent_count = self.count_agents_by_owner(&user.id).await?;
        let harness_provisioning = self
            .find_harness_provisioning_by_user_id(&user.id)
            .await?
            .map(Into::into);
        Ok(UserSummaryRecord {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            role: user.role,
            enabled: user.enabled,
            created_at: user.created_at,
            updated_at: user.updated_at,
            last_login_at: user.last_login_at,
            agent_count,
            harness_provisioning,
        })
    }

    pub async fn insert_user_record(&self, user: &UserRecord) -> Result<(), String> {
        sqlx::query(
            r#"INSERT INTO users
            (id, username, display_name, password_hash, role, enabled, created_at, updated_at, last_login_at, data)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(&user.role)
        .bind(user.enabled)
        .bind(timestamp(&user.created_at)?)
        .bind(timestamp(&user.updated_at)?)
        .bind(optional_timestamp(user.last_login_at.as_deref())?)
        .bind(json(user)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn update_user_record(&self, user: &UserRecord) -> Result<(), String> {
        sqlx::query(
            r#"UPDATE users SET username=$2, display_name=$3, password_hash=$4, role=$5,
            enabled=$6, updated_at=$7, last_login_at=$8, data=$9 WHERE id=$1"#,
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(&user.role)
        .bind(user.enabled)
        .bind(timestamp(&user.updated_at)?)
        .bind(optional_timestamp(user.last_login_at.as_deref())?)
        .bind(json(user)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn touch_user_last_login(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query(
            r#"UPDATE users SET last_login_at=$2, updated_at=$2,
            data=jsonb_set(jsonb_set(data,'{last_login_at}',to_jsonb($3::text)),'{updated_at}',to_jsonb($3::text))
            WHERE id=$1"#,
        )
        .bind(id)
        .bind(timestamp(&now)?)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn count_enabled_super_admins(&self) -> Result<i64, String> {
        sqlx::query_scalar("SELECT count(*) FROM users WHERE enabled=TRUE AND role=$1")
            .bind(USER_ROLE_SUPER_ADMIN)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)
    }

    pub async fn list_agent_accounts(&self) -> Result<Vec<AgentAccountListItem>, String> {
        self.list_agent_accounts_inner(None).await
    }

    pub async fn list_agent_accounts_by_owner(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<AgentAccountListItem>, String> {
        self.list_agent_accounts_inner(Some(owner_user_id)).await
    }

    async fn list_agent_accounts_inner(
        &self,
        owner_user_id: Option<&str>,
    ) -> Result<Vec<AgentAccountListItem>, String> {
        let agents: Vec<AgentAccountRecord> = match owner_user_id {
            Some(owner) => fetch_all(
                sqlx::query_scalar("SELECT data FROM agent_accounts WHERE owner_user_id=$1 ORDER BY updated_at DESC, created_at DESC, id").bind(owner),
                &self.pool,
            ).await?,
            None => fetch_all(
                sqlx::query_scalar("SELECT data FROM agent_accounts ORDER BY updated_at DESC, created_at DESC, id"),
                &self.pool,
            ).await?,
        };
        let owner_ids = agents
            .iter()
            .map(|agent| agent.owner_user_id.clone())
            .collect::<Vec<_>>();
        let owners = self
            .list_user_options(Some(&owner_ids))
            .await?
            .into_iter()
            .map(|owner| (owner.id.clone(), owner))
            .collect::<HashMap<_, _>>();
        Ok(agents
            .into_iter()
            .filter_map(|agent| {
                let owner = owners.get(&agent.owner_user_id)?;
                Some(AgentAccountListItem {
                    id: agent.id,
                    username: agent.username,
                    display_name: agent.display_name,
                    owner_user_id: agent.owner_user_id,
                    owner_username: owner.username.clone(),
                    owner_display_name: owner.display_name.clone(),
                    enabled: agent.enabled,
                    created_at: agent.created_at,
                    updated_at: agent.updated_at,
                    last_login_at: agent.last_login_at,
                })
            })
            .collect())
    }

    pub async fn find_agent_by_id(&self, id: &str) -> Result<Option<AgentAccountRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM agent_accounts WHERE id=$1").bind(id),
            &self.pool,
        )
        .await
    }

    pub async fn find_agent_by_username(
        &self,
        username: &str,
    ) -> Result<Option<AgentAccountRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM agent_accounts WHERE username=$1").bind(username),
            &self.pool,
        )
        .await
    }

    pub async fn insert_agent_record(&self, agent: &AgentAccountRecord) -> Result<(), String> {
        sqlx::query(
            r#"INSERT INTO agent_accounts
            (id,username,owner_user_id,enabled,created_at,updated_at,last_login_at,data)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
        )
        .bind(&agent.id)
        .bind(&agent.username)
        .bind(&agent.owner_user_id)
        .bind(agent.enabled)
        .bind(timestamp(&agent.created_at)?)
        .bind(timestamp(&agent.updated_at)?)
        .bind(optional_timestamp(agent.last_login_at.as_deref())?)
        .bind(json(agent)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn update_agent_record(&self, agent: &AgentAccountRecord) -> Result<(), String> {
        sqlx::query(
            r#"UPDATE agent_accounts SET username=$2,owner_user_id=$3,enabled=$4,
            updated_at=$5,last_login_at=$6,data=$7 WHERE id=$1"#,
        )
        .bind(&agent.id)
        .bind(&agent.username)
        .bind(&agent.owner_user_id)
        .bind(agent.enabled)
        .bind(timestamp(&agent.updated_at)?)
        .bind(optional_timestamp(agent.last_login_at.as_deref())?)
        .bind(json(agent)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn touch_agent_last_login(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query(r#"UPDATE agent_accounts SET last_login_at=$2,updated_at=$2,
            data=jsonb_set(jsonb_set(data,'{last_login_at}',to_jsonb($3::text)),'{updated_at}',to_jsonb($3::text)) WHERE id=$1"#)
            .bind(id).bind(timestamp(&now)?).bind(&now).execute(&self.pool).await
            .map(|_| ()).map_err(db_error)
    }

    pub async fn revoke_token(
        &self,
        jti: &str,
        subject_id: &str,
        expires_at_unix: i64,
    ) -> Result<(), String> {
        sqlx::query(r#"INSERT INTO revoked_tokens (jti,subject_id,revoked_at,expires_at)
            VALUES ($1,$2,now(),$3) ON CONFLICT (jti) DO UPDATE SET
            subject_id=EXCLUDED.subject_id,revoked_at=EXCLUDED.revoked_at,expires_at=EXCLUDED.expires_at"#)
            .bind(jti).bind(subject_id).bind(expires_at_unix).execute(&self.pool).await
            .map(|_| ()).map_err(db_error)
    }

    pub async fn is_token_revoked(&self, jti: &str) -> Result<bool, String> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM revoked_tokens WHERE jti=$1 AND expires_at>$2)",
        )
        .bind(jti)
        .bind(Utc::now().timestamp())
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)
    }

    pub async fn count_agents_by_owner(&self, owner_user_id: &str) -> Result<i64, String> {
        sqlx::query_scalar("SELECT count(*) FROM agent_accounts WHERE owner_user_id=$1")
            .bind(owner_user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)
    }

    pub async fn username_exists_elsewhere(
        &self,
        username: &str,
        current_user_id: Option<&str>,
    ) -> Result<bool, String> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE username=$1 AND ($2::text IS NULL OR id<>$2))",
        )
        .bind(username)
        .bind(current_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)
    }

    pub async fn find_harness_provisioning_by_user_id(
        &self,
        user_id: &str,
    ) -> Result<Option<HarnessProvisioningRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM harness_provisioning WHERE user_id=$1")
                .bind(user_id),
            &self.pool,
        )
        .await
    }

    pub async fn save_harness_provisioning(
        &self,
        record: &HarnessProvisioningRecord,
    ) -> Result<HarnessProvisioningRecord, String> {
        sqlx::query(r#"INSERT INTO harness_provisioning (user_id,status,updated_at,data) VALUES ($1,$2,$3,$4)
            ON CONFLICT (user_id) DO UPDATE SET status=EXCLUDED.status,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data"#)
            .bind(&record.user_id).bind(&record.status).bind(timestamp(&record.updated_at)?).bind(json(record)?)
            .execute(&self.pool).await.map_err(db_error)?;
        Ok(record.clone())
    }

    pub async fn find_registration_email_code(
        &self,
        email: &str,
    ) -> Result<Option<RegistrationEmailCodeRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM registration_email_codes WHERE email=$1")
                .bind(email),
            &self.pool,
        )
        .await
    }

    pub async fn save_registration_email_code(
        &self,
        record: &RegistrationEmailCodeRecord,
    ) -> Result<(), String> {
        sqlx::query(r#"INSERT INTO registration_email_codes (email,expires_at,consumed_at,updated_at,data)
            VALUES ($1,$2,$3,$4,$5) ON CONFLICT (email) DO UPDATE SET expires_at=EXCLUDED.expires_at,
            consumed_at=EXCLUDED.consumed_at,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data"#)
            .bind(&record.email).bind(record.expires_at_unix).bind(optional_timestamp(record.consumed_at.as_deref())?)
            .bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn mark_registration_email_code_consumed(&self, email: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query(r#"UPDATE registration_email_codes SET consumed_at=$2,updated_at=$2,
            data=jsonb_set(jsonb_set(data,'{consumed_at}',to_jsonb($3::text)),'{updated_at}',to_jsonb($3::text)) WHERE email=$1"#)
            .bind(email).bind(timestamp(&now)?).bind(&now).execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn insert_local_connector_auth_ticket(
        &self,
        record: &LocalConnectorAuthTicketRecord,
    ) -> Result<(), String> {
        sqlx::query(r#"INSERT INTO local_connector_auth_tickets
            (id,ticket_hash,user_id,expires_at,consumed_at,updated_at,data) VALUES ($1,$2,$3,$4,$5,$6,$7)"#)
            .bind(&record.id).bind(&record.ticket_hash).bind(&record.user_id).bind(record.expires_at_unix)
            .bind(optional_timestamp(record.consumed_at.as_deref())?).bind(timestamp(&record.updated_at)?).bind(json(record)?)
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn consume_local_connector_auth_ticket(
        &self,
        ticket_hash: &str,
        now_unix: i64,
        now: &str,
    ) -> Result<Option<LocalConnectorAuthTicketRecord>, String> {
        fetch_optional(sqlx::query_scalar(r#"UPDATE local_connector_auth_tickets SET consumed_at=$3,updated_at=$3,
            data=jsonb_set(jsonb_set(data,'{consumed_at}',to_jsonb($4::text)),'{updated_at}',to_jsonb($4::text))
            WHERE ticket_hash=$1 AND consumed_at IS NULL AND expires_at>$2 RETURNING data"#)
            .bind(ticket_hash).bind(now_unix).bind(timestamp(now)?).bind(now), &self.pool).await
    }

    pub async fn list_invite_codes(&self) -> Result<Vec<InviteCodePublicRecord>, String> {
        let rows: Vec<InviteCodeRecord> = fetch_all(
            sqlx::query_scalar("SELECT data FROM invite_codes ORDER BY created_at DESC,id"),
            &self.pool,
        )
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn insert_invite_code(
        &self,
        record: &InviteCodeRecord,
    ) -> Result<InviteCodePublicRecord, String> {
        sqlx::query(r#"INSERT INTO invite_codes
            (id,code_hash,created_by_user_id,max_uses,used_count,expires_at,revoked_at,created_at,updated_at,data)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#)
            .bind(&record.id).bind(&record.code_hash).bind(&record.created_by_user_id).bind(record.max_uses)
            .bind(record.used_count).bind(record.expires_at_unix).bind(optional_timestamp(record.revoked_at.as_deref())?)
            .bind(timestamp(&record.created_at)?).bind(timestamp(&record.updated_at)?).bind(json(record)?)
            .execute(&self.pool).await.map_err(db_error)?;
        Ok(record.clone().into())
    }

    pub async fn find_invite_code_by_hash(
        &self,
        code_hash: &str,
    ) -> Result<Option<InviteCodeRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM invite_codes WHERE code_hash=$1").bind(code_hash),
            &self.pool,
        )
        .await
    }

    pub async fn find_invite_code_by_id(
        &self,
        id: &str,
    ) -> Result<Option<InviteCodeRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM invite_codes WHERE id=$1").bind(id),
            &self.pool,
        )
        .await
    }

    pub async fn update_invite_code(&self, record: &InviteCodeRecord) -> Result<(), String> {
        sqlx::query(
            r#"UPDATE invite_codes SET code_hash=$2,max_uses=$3,used_count=$4,expires_at=$5,
            revoked_at=$6,updated_at=$7,data=$8 WHERE id=$1"#,
        )
        .bind(&record.id)
        .bind(&record.code_hash)
        .bind(record.max_uses)
        .bind(record.used_count)
        .bind(record.expires_at_unix)
        .bind(optional_timestamp(record.revoked_at.as_deref())?)
        .bind(timestamp(&record.updated_at)?)
        .bind(json(record)?)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn consume_invite_code(
        &self,
        id: &str,
        now_unix: i64,
        now: &str,
    ) -> Result<bool, String> {
        sqlx::query(r#"UPDATE invite_codes SET used_count=used_count+1,updated_at=$3,
            data=jsonb_set(jsonb_set(jsonb_set(data,'{used_count}',to_jsonb(used_count+1)),
            '{last_used_at}',to_jsonb($4::text)),'{updated_at}',to_jsonb($4::text))
            WHERE id=$1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>$2) AND used_count<max_uses"#)
            .bind(id).bind(now_unix).bind(timestamp(now)?).bind(now).execute(&self.pool).await
            .map(|result| result.rows_affected()==1).map_err(db_error)
    }
}

#[derive(Clone, Copy)]
struct SuperAdminBootstrapConfig<'a> {
    username: &'a str,
    password: &'a str,
    display_name: &'a str,
    allow_empty_database_admin_creation: bool,
}

impl<'a> From<&'a AppConfig> for SuperAdminBootstrapConfig<'a> {
    fn from(config: &'a AppConfig) -> Self {
        Self {
            username: config.super_admin_username.as_str(),
            password: config.super_admin_password.as_str(),
            display_name: config.super_admin_display_name.as_str(),
            allow_empty_database_admin_creation: config.allow_empty_database_admin_creation,
        }
    }
}

fn ensure_empty_database_bootstrap_allowed(
    production: bool,
    allow_empty_database_admin_creation: bool,
) -> Result<(), String> {
    if production {
        return Err(
            "User Service refuses to start because the PostgreSQL users table is empty in production; migrate users into PostgreSQL before starting User Service"
                .to_string(),
        );
    }
    if !allow_empty_database_admin_creation {
        return Err(
            "User Service refuses to start because the PostgreSQL users table is empty and automatic administrator creation is disabled; migrate users first or explicitly enable USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION for local development"
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|err| format!("invalid RFC3339 timestamp {value:?}: {err}"))
}

pub(crate) fn optional_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    value.map(timestamp).transpose()
}

pub(crate) fn json<T: Serialize>(value: &T) -> Result<Json<Value>, String> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|err| err.to_string())
}

pub(crate) async fn fetch_all<'q, T>(
    query: sqlx::query::QueryScalar<'q, sqlx::Postgres, Json<Value>, sqlx::postgres::PgArguments>,
    pool: &chatos_postgres::PgPool,
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

pub(crate) async fn fetch_optional<'q, T>(
    query: sqlx::query::QueryScalar<'q, sqlx::Postgres, Json<Value>, sqlx::postgres::PgArguments>,
    pool: &chatos_postgres::PgPool,
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

pub(crate) fn db_error(error: sqlx::Error) -> String {
    error.to_string()
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests;
