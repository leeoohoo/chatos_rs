// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{TimeZone, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::types::Json;

use crate::models::{
    ClientSessionRecord, UserExternalIdentityRecord, WeChatBindTicketRecord,
    WECHAT_BIND_STATUS_CLAIMED, WECHAT_BIND_STATUS_CONFIRMED, WECHAT_BIND_STATUS_CONSUMED,
    WECHAT_BIND_STATUS_EXPIRED, WECHAT_BIND_STATUS_ISSUED,
};

use super::{db_error, json, optional_timestamp, timestamp, AppStore};

#[derive(Debug)]
pub enum BindExternalIdentityResult {
    Bound(Box<UserExternalIdentityRecord>),
    Conflict,
}

impl AppStore {
    pub async fn find_active_external_identity_by_subject(
        &self,
        provider: &str,
        app_id: &str,
        open_id_hash: &str,
    ) -> Result<Option<UserExternalIdentityRecord>, String> {
        decode_optional(sqlx::query_scalar(
            "SELECT data FROM user_external_identities WHERE provider=$1 AND app_id=$2 AND open_id_hash=$3 AND revoked_at IS NULL",
        ).bind(provider).bind(app_id).bind(open_id_hash).fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub async fn find_active_external_identity_by_id(
        &self,
        id: &str,
    ) -> Result<Option<UserExternalIdentityRecord>, String> {
        decode_optional(
            sqlx::query_scalar(
                "SELECT data FROM user_external_identities WHERE id=$1 AND revoked_at IS NULL",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn find_active_external_identity_for_user(
        &self,
        user_id: &str,
        provider: &str,
        app_id: &str,
    ) -> Result<Option<UserExternalIdentityRecord>, String> {
        decode_optional(sqlx::query_scalar(
            "SELECT data FROM user_external_identities WHERE user_id=$1 AND provider=$2 AND app_id=$3 AND revoked_at IS NULL",
        ).bind(user_id).bind(provider).bind(app_id).fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub async fn bind_external_identity(
        &self,
        record: &UserExternalIdentityRecord,
    ) -> Result<BindExternalIdentityResult, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let mut lock_keys = [
            format!(
                "identity-subject:{}:{}:{}",
                record.provider, record.app_id, record.open_id_hash
            ),
            format!(
                "identity-user:{}:{}:{}",
                record.user_id, record.provider, record.app_id
            ),
        ];
        lock_keys.sort();
        for key in lock_keys {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(key)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
        }

        let by_subject: Option<UserExternalIdentityRecord> = decode_optional(sqlx::query_scalar(
            "SELECT data FROM user_external_identities WHERE provider=$1 AND app_id=$2 AND open_id_hash=$3 AND revoked_at IS NULL FOR UPDATE",
        ).bind(&record.provider).bind(&record.app_id).bind(&record.open_id_hash)
            .fetch_optional(&mut *tx).await.map_err(db_error)?)?;
        if let Some(existing) = by_subject {
            if existing.user_id != record.user_id {
                return Ok(BindExternalIdentityResult::Conflict);
            }
            let updated = update_bound_identity(&mut tx, existing, record).await?;
            tx.commit().await.map_err(db_error)?;
            return Ok(BindExternalIdentityResult::Bound(Box::new(updated)));
        }

        let by_user: Option<UserExternalIdentityRecord> = decode_optional(sqlx::query_scalar(
            "SELECT data FROM user_external_identities WHERE user_id=$1 AND provider=$2 AND app_id=$3 AND revoked_at IS NULL FOR UPDATE",
        ).bind(&record.user_id).bind(&record.provider).bind(&record.app_id)
            .fetch_optional(&mut *tx).await.map_err(db_error)?)?;
        if let Some(existing) = by_user {
            if existing.open_id_hash != record.open_id_hash {
                return Ok(BindExternalIdentityResult::Conflict);
            }
            let updated = update_bound_identity(&mut tx, existing, record).await?;
            tx.commit().await.map_err(db_error)?;
            return Ok(BindExternalIdentityResult::Bound(Box::new(updated)));
        }

        let revoked: Option<UserExternalIdentityRecord> = decode_optional(sqlx::query_scalar(
            "SELECT data FROM user_external_identities WHERE user_id=$1 AND provider=$2 AND app_id=$3 AND open_id_hash=$4 AND revoked_at IS NOT NULL ORDER BY updated_at DESC LIMIT 1 FOR UPDATE",
        ).bind(&record.user_id).bind(&record.provider).bind(&record.app_id).bind(&record.open_id_hash)
            .fetch_optional(&mut *tx).await.map_err(db_error)?)?;

        let result = if let Some(mut revived) = revoked {
            revived.union_id_hash.clone_from(&record.union_id_hash);
            revived
                .companion_device_id
                .clone_from(&record.companion_device_id);
            revived
                .companion_device_public_key
                .clone_from(&record.companion_device_public_key);
            revived.updated_at.clone_from(&record.updated_at);
            revived.last_login_at.clone_from(&record.last_login_at);
            revived.revoked_at = None;
            sqlx::query("UPDATE user_external_identities SET union_id_hash=$2,revoked_at=NULL,updated_at=$3,data=$4 WHERE id=$1 AND revoked_at IS NOT NULL")
                .bind(&revived.id).bind(&revived.union_id_hash).bind(timestamp(&revived.updated_at)?)
                .bind(json(&revived)?).execute(&mut *tx).await.map_err(db_error)?;
            revived
        } else {
            sqlx::query("INSERT INTO user_external_identities (id,user_id,provider,app_id,open_id_hash,union_id_hash,revoked_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                .bind(&record.id).bind(&record.user_id).bind(&record.provider).bind(&record.app_id)
                .bind(&record.open_id_hash).bind(&record.union_id_hash)
                .bind(optional_timestamp(record.revoked_at.as_deref())?).bind(timestamp(&record.updated_at)?)
                .bind(json(record)?).execute(&mut *tx).await.map_err(db_error)?;
            record.clone()
        };
        tx.commit().await.map_err(db_error)?;
        Ok(BindExternalIdentityResult::Bound(Box::new(result)))
    }

    pub async fn touch_external_identity_login(&self, id: &str, now: &str) -> Result<(), String> {
        sqlx::query("UPDATE user_external_identities SET updated_at=$2,data=data || jsonb_build_object('last_login_at',$3::text,'updated_at',$3::text) WHERE id=$1 AND revoked_at IS NULL")
            .bind(id).bind(timestamp(now)?).bind(now).execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn revoke_external_identity(
        &self,
        user_id: &str,
        provider: &str,
        app_id: &str,
        now: &str,
    ) -> Result<Option<UserExternalIdentityRecord>, String> {
        decode_optional(sqlx::query_scalar(
            "UPDATE user_external_identities SET revoked_at=$4,updated_at=$4,data=data || jsonb_build_object('revoked_at',$5::text,'updated_at',$5::text) WHERE user_id=$1 AND provider=$2 AND app_id=$3 AND revoked_at IS NULL RETURNING data",
        ).bind(user_id).bind(provider).bind(app_id).bind(timestamp(now)?).bind(now)
            .fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub async fn insert_wechat_bind_ticket(
        &self,
        record: &WeChatBindTicketRecord,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO wechat_bind_tickets (id,ticket_hash,user_id,app_id,status,claim_id,expires_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&record.id).bind(&record.ticket_hash).bind(&record.user_id).bind(&record.app_id)
            .bind(&record.status).bind(&record.claim_id).bind(record.expires_at_unix)
            .bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await
            .map(|_| ()).map_err(db_error)
    }

    pub async fn expire_open_wechat_bind_tickets_for_user(
        &self,
        user_id: &str,
        app_id: &str,
        now: &str,
    ) -> Result<(), String> {
        sqlx::query("UPDATE wechat_bind_tickets SET status=$3,updated_at=$4,data=data || jsonb_build_object('status',$3::text,'updated_at',$5::text) WHERE user_id=$1 AND app_id=$2 AND status=ANY($6)")
            .bind(user_id).bind(app_id).bind(WECHAT_BIND_STATUS_EXPIRED).bind(timestamp(now)?).bind(now)
            .bind(vec![WECHAT_BIND_STATUS_ISSUED, WECHAT_BIND_STATUS_CLAIMED])
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn find_wechat_bind_ticket_for_user(
        &self,
        ticket_id: &str,
        user_id: &str,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        decode_optional(
            sqlx::query_scalar("SELECT data FROM wechat_bind_tickets WHERE id=$1 AND user_id=$2")
                .bind(ticket_id)
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_error)?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn claim_wechat_bind_ticket(
        &self,
        ticket_hash: &str,
        app_id: &str,
        open_id_hash: &str,
        union_id_hash: Option<&str>,
        device_id: &str,
        device_public_key: &str,
        claim_id: &str,
        claim_secret_hash: &str,
        now_unix: i64,
        now: &str,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        decode_optional(sqlx::query_scalar(
            "UPDATE wechat_bind_tickets SET status=$3,claim_id=$4,updated_at=$5,data=data || jsonb_build_object('status',$3::text,'claimed_open_id_hash',$6::text,'claimed_union_id_hash',$7::text,'claimed_device_id',$8::text,'claimed_device_public_key',$9::text,'claim_id',$4::text,'claim_secret_hash',$10::text,'claimed_at',$11::text,'updated_at',$11::text) WHERE ticket_hash=$1 AND app_id=$2 AND status=$12 AND expires_at>$13 RETURNING data",
        ).bind(ticket_hash).bind(app_id).bind(WECHAT_BIND_STATUS_CLAIMED).bind(claim_id)
            .bind(timestamp(now)?).bind(open_id_hash).bind(union_id_hash).bind(device_id)
            .bind(device_public_key).bind(claim_secret_hash).bind(now).bind(WECHAT_BIND_STATUS_ISSUED)
            .bind(now_unix).fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub async fn confirm_wechat_bind_ticket(
        &self,
        ticket_id: &str,
        user_id: &str,
        external_identity_id: &str,
        now_unix: i64,
        now: &str,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        decode_optional(sqlx::query_scalar(
            "UPDATE wechat_bind_tickets SET status=$3,updated_at=$4,data=data || jsonb_build_object('status',$3::text,'confirmed_external_identity_id',$5::text,'confirmed_at',$6::text,'updated_at',$6::text) WHERE id=$1 AND user_id=$2 AND status=$7 AND expires_at>$8 RETURNING data",
        ).bind(ticket_id).bind(user_id).bind(WECHAT_BIND_STATUS_CONFIRMED).bind(timestamp(now)?)
            .bind(external_identity_id).bind(now).bind(WECHAT_BIND_STATUS_CLAIMED).bind(now_unix)
            .fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub async fn consume_confirmed_wechat_claim(
        &self,
        claim_id: &str,
        claim_secret_hash: &str,
        now_unix: i64,
        now: &str,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        decode_optional(sqlx::query_scalar(
            "UPDATE wechat_bind_tickets SET status=$3,updated_at=$4,data=data || jsonb_build_object('status',$3::text,'consumed_at',$5::text,'updated_at',$5::text) WHERE claim_id=$1 AND data->>'claim_secret_hash'=$2 AND status=$6 AND expires_at>$7 RETURNING data",
        ).bind(claim_id).bind(claim_secret_hash).bind(WECHAT_BIND_STATUS_CONSUMED).bind(timestamp(now)?)
            .bind(now).bind(WECHAT_BIND_STATUS_CONFIRMED).bind(now_unix)
            .fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub async fn find_wechat_claim(
        &self,
        claim_id: &str,
        claim_secret_hash: &str,
        now_unix: i64,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        decode_optional(sqlx::query_scalar("SELECT data FROM wechat_bind_tickets WHERE claim_id=$1 AND data->>'claim_secret_hash'=$2 AND expires_at>$3")
            .bind(claim_id).bind(claim_secret_hash).bind(now_unix).fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub async fn insert_client_session(&self, record: &ClientSessionRecord) -> Result<(), String> {
        sqlx::query("INSERT INTO client_sessions (id,user_id,client_type,external_identity_id,token_jti,expires_at,revoked_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&record.id).bind(&record.user_id).bind(&record.client_type).bind(&record.external_identity_id)
            .bind(&record.token_jti).bind(record.expires_at_unix).bind(optional_timestamp(record.revoked_at.as_deref())?)
            .bind(timestamp(&record.updated_at)?).bind(json(record)?).execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn find_client_session_by_jti(
        &self,
        token_jti: &str,
    ) -> Result<Option<ClientSessionRecord>, String> {
        decode_optional(
            sqlx::query_scalar("SELECT data FROM client_sessions WHERE token_jti=$1")
                .bind(token_jti)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_error)?,
        )
    }

    pub async fn consume_device_proof_nonce(
        &self,
        session_id: &str,
        nonce: &str,
        expires_at_unix_ms: i64,
    ) -> Result<bool, String> {
        let expires_at = Utc
            .timestamp_millis_opt(expires_at_unix_ms)
            .single()
            .ok_or_else(|| format!("invalid nonce expiry timestamp: {expires_at_unix_ms}"))?;
        sqlx::query("INSERT INTO device_proof_nonces(id,expires_at) VALUES($1,$2) ON CONFLICT(id) DO NOTHING")
            .bind(format!("{session_id}:{nonce}")).bind(expires_at).execute(&self.pool).await
            .map(|result| result.rows_affected() == 1).map_err(db_error)
    }

    pub async fn list_client_sessions(
        &self,
        user_id: &str,
    ) -> Result<Vec<ClientSessionRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM client_sessions WHERE user_id=$1 ORDER BY (data->>'created_at')::timestamptz DESC LIMIT 100")
            .bind(user_id).fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn revoke_client_session(
        &self,
        session_id: &str,
        user_id: &str,
        revoked_by: &str,
        now: &str,
    ) -> Result<Option<ClientSessionRecord>, String> {
        decode_optional(sqlx::query_scalar(
            "UPDATE client_sessions SET revoked_at=$3,updated_at=$3,data=data || jsonb_build_object('revoked_at',$4::text,'revoked_by',$5::text,'updated_at',$4::text) WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL RETURNING data",
        ).bind(session_id).bind(user_id).bind(timestamp(now)?).bind(now).bind(revoked_by)
            .fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub async fn revoke_client_session_by_jti(
        &self,
        token_jti: &str,
        revoked_by: &str,
        now: &str,
    ) -> Result<(), String> {
        sqlx::query("UPDATE client_sessions SET revoked_at=$2,updated_at=$2,data=data || jsonb_build_object('revoked_at',$3::text,'revoked_by',$4::text,'updated_at',$3::text) WHERE token_jti=$1 AND revoked_at IS NULL")
            .bind(token_jti).bind(timestamp(now)?).bind(now).bind(revoked_by)
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn touch_client_session(&self, token_jti: &str, now: &str) -> Result<(), String> {
        sqlx::query("UPDATE client_sessions SET updated_at=$2,data=data || jsonb_build_object('last_seen_at',$3::text,'updated_at',$3::text) WHERE token_jti=$1 AND revoked_at IS NULL AND expires_at>$4")
            .bind(token_jti).bind(timestamp(now)?).bind(now).bind(Utc::now().timestamp())
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn revoke_client_sessions_for_identity(
        &self,
        identity_id: &str,
        revoked_by: &str,
        now: &str,
    ) -> Result<Vec<ClientSessionRecord>, String> {
        decode_all(sqlx::query_scalar(
            "UPDATE client_sessions SET revoked_at=$2,updated_at=$2,data=data || jsonb_build_object('revoked_at',$3::text,'revoked_by',$4::text,'updated_at',$3::text) WHERE external_identity_id=$1 AND revoked_at IS NULL RETURNING data",
        ).bind(identity_id).bind(timestamp(now)?).bind(now).bind(revoked_by)
            .fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn is_client_session_invalid(
        &self,
        token_jti: &str,
        require_record: bool,
    ) -> Result<bool, String> {
        let row = sqlx::query_as::<_, (Option<chrono::DateTime<Utc>>, i64, Option<String>, bool)>(
            "SELECT session.revoked_at,session.expires_at,session.external_identity_id,identity.id IS NOT NULL FROM client_sessions session LEFT JOIN user_external_identities identity ON identity.id=session.external_identity_id AND identity.revoked_at IS NULL WHERE session.token_jti=$1",
        ).bind(token_jti).fetch_optional(&self.pool).await.map_err(db_error)?;
        let Some((revoked_at, expires_at, identity_id, identity_active)) = row else {
            return Ok(require_record);
        };
        Ok(revoked_at.is_some()
            || expires_at <= Utc::now().timestamp()
            || (identity_id.is_some() && !identity_active))
    }
}

async fn update_bound_identity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    mut existing: UserExternalIdentityRecord,
    requested: &UserExternalIdentityRecord,
) -> Result<UserExternalIdentityRecord, String> {
    existing.union_id_hash.clone_from(&requested.union_id_hash);
    existing
        .companion_device_id
        .clone_from(&requested.companion_device_id);
    existing
        .companion_device_public_key
        .clone_from(&requested.companion_device_public_key);
    existing.updated_at.clone_from(&requested.updated_at);
    sqlx::query("UPDATE user_external_identities SET union_id_hash=$2,updated_at=$3,data=$4 WHERE id=$1 AND revoked_at IS NULL")
        .bind(&existing.id).bind(&existing.union_id_hash).bind(timestamp(&existing.updated_at)?)
        .bind(json(&existing)?).execute(&mut **tx).await.map_err(db_error)?;
    Ok(existing)
}

fn decode_optional<T: DeserializeOwned>(value: Option<Json<Value>>) -> Result<Option<T>, String> {
    value
        .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
        .transpose()
}

fn decode_all<T: DeserializeOwned>(values: Vec<Json<Value>>) -> Result<Vec<T>, String> {
    values
        .into_iter()
        .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
        .collect()
}
