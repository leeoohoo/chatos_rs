// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sqlx::types::Json;

use super::{db_error, fetch_optional, json, optional_timestamp, timestamp, AppStore};
use crate::models::{InviteCodeRecord, RegistrationEmailCodeRecord, UserRecord};

#[derive(Debug, Clone)]
pub struct RegistrationEmailCodeReservation {
    pub record: RegistrationEmailCodeRecord,
    pub previous: Option<RegistrationEmailCodeRecord>,
}

#[derive(Debug)]
pub enum RegistrationEmailCodeReservationError {
    ResendTooSoon,
    HourlyLimitReached,
    Store(String),
}

#[derive(Debug)]
pub enum RegistrationTransactionError {
    InvalidVerificationCode,
    InvalidInvite,
    EmailAlreadyRegistered,
    Store(String),
}

impl AppStore {
    #[allow(clippy::too_many_arguments)]
    pub async fn register_user_with_invite_and_email_code(
        &self,
        user: &UserRecord,
        invite_code_hash: &str,
        expected_code_hash: &str,
        now_unix: i64,
        now: &str,
        max_attempts: i64,
    ) -> Result<(), RegistrationTransactionError> {
        let store_error = |error| RegistrationTransactionError::Store(db_error(error));
        let mut tx = self.pool.begin().await.map_err(store_error)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(&user.username)
            .execute(&mut *tx)
            .await
            .map_err(store_error)?;

        let verification = sqlx::query_scalar::<_, Json<Value>>(
            "SELECT data FROM registration_email_codes WHERE email=$1 FOR UPDATE",
        )
        .bind(&user.username)
        .fetch_optional(&mut *tx)
        .await
        .map_err(store_error)?
        .map(|Json(value)| serde_json::from_value::<RegistrationEmailCodeRecord>(value))
        .transpose()
        .map_err(|error| RegistrationTransactionError::Store(error.to_string()))?;
        let Some(mut verification) = verification else {
            return Err(RegistrationTransactionError::InvalidVerificationCode);
        };
        let verification_valid = verification.consumed_at.is_none()
            && verification.expires_at_unix >= now_unix
            && verification.invite_code_hash == invite_code_hash
            && verification.attempts < max_attempts
            && verification.code_hash == expected_code_hash;
        if !verification_valid {
            if verification.consumed_at.is_none()
                && verification.expires_at_unix >= now_unix
                && verification.invite_code_hash == invite_code_hash
                && verification.attempts < max_attempts
            {
                verification.attempts += 1;
                verification.updated_at = now.to_string();
                sqlx::query(
                    r#"UPDATE registration_email_codes SET updated_at=$2,data=$3 WHERE email=$1"#,
                )
                .bind(&verification.email)
                .bind(timestamp(now).map_err(RegistrationTransactionError::Store)?)
                .bind(json(&verification).map_err(RegistrationTransactionError::Store)?)
                .execute(&mut *tx)
                .await
                .map_err(store_error)?;
                tx.commit().await.map_err(store_error)?;
            }
            return Err(RegistrationTransactionError::InvalidVerificationCode);
        }

        let invite = sqlx::query_scalar::<_, Json<Value>>(
            "SELECT data FROM invite_codes WHERE code_hash=$1 FOR UPDATE",
        )
        .bind(invite_code_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(store_error)?
        .map(|Json(value)| serde_json::from_value::<InviteCodeRecord>(value))
        .transpose()
        .map_err(|error| RegistrationTransactionError::Store(error.to_string()))?;
        let Some(mut invite) = invite else {
            return Err(RegistrationTransactionError::InvalidInvite);
        };
        if invite.revoked_at.is_some()
            || invite
                .expires_at_unix
                .is_some_and(|expires_at| expires_at < now_unix)
            || invite.used_count >= invite.max_uses
        {
            return Err(RegistrationTransactionError::InvalidInvite);
        }

        let insert_result = sqlx::query(
            r#"INSERT INTO users
            (id,username,display_name,password_hash,credential_version,role,enabled,created_at,updated_at,last_login_at,data)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(user.credential_version)
        .bind(&user.role)
        .bind(user.enabled)
        .bind(timestamp(&user.created_at).map_err(RegistrationTransactionError::Store)?)
        .bind(timestamp(&user.updated_at).map_err(RegistrationTransactionError::Store)?)
        .bind(optional_timestamp(user.last_login_at.as_deref()).map_err(RegistrationTransactionError::Store)?)
        .bind(json(user).map_err(RegistrationTransactionError::Store)?)
        .execute(&mut *tx)
        .await;
        if let Err(sqlx::Error::Database(error)) = &insert_result {
            if error.is_unique_violation() {
                return Err(RegistrationTransactionError::EmailAlreadyRegistered);
            }
        }
        insert_result.map_err(store_error)?;

        invite.used_count += 1;
        invite.last_used_at = Some(now.to_string());
        invite.updated_at = now.to_string();
        sqlx::query(r#"UPDATE invite_codes SET used_count=$2,updated_at=$3,data=$4 WHERE id=$1"#)
            .bind(&invite.id)
            .bind(invite.used_count)
            .bind(timestamp(now).map_err(RegistrationTransactionError::Store)?)
            .bind(json(&invite).map_err(RegistrationTransactionError::Store)?)
            .execute(&mut *tx)
            .await
            .map_err(store_error)?;

        verification.consumed_at = Some(now.to_string());
        verification.updated_at = now.to_string();
        sqlx::query(
            r#"UPDATE registration_email_codes SET consumed_at=$2,updated_at=$2,data=$3 WHERE email=$1"#,
        )
        .bind(&verification.email)
        .bind(timestamp(now).map_err(RegistrationTransactionError::Store)?)
        .bind(json(&verification).map_err(RegistrationTransactionError::Store)?)
        .execute(&mut *tx)
        .await
        .map_err(store_error)?;
        tx.commit().await.map_err(store_error)
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

    #[allow(clippy::too_many_arguments)]
    pub async fn reserve_registration_email_code_send(
        &self,
        email: &str,
        code_hash: String,
        invite_code_hash: String,
        now_unix: i64,
        now: String,
        ttl_seconds: i64,
        resend_seconds: i64,
        hourly_limit: i64,
    ) -> Result<RegistrationEmailCodeReservation, RegistrationEmailCodeReservationError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| RegistrationEmailCodeReservationError::Store(db_error(error)))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(email)
            .execute(&mut *tx)
            .await
            .map_err(|error| RegistrationEmailCodeReservationError::Store(db_error(error)))?;
        let previous: Option<RegistrationEmailCodeRecord> = sqlx::query_scalar::<_, Json<Value>>(
            "SELECT data FROM registration_email_codes WHERE email=$1 FOR UPDATE",
        )
        .bind(email)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| RegistrationEmailCodeReservationError::Store(db_error(error)))?
        .map(|Json(value)| serde_json::from_value(value).map_err(|error| error.to_string()))
        .transpose()
        .map_err(RegistrationEmailCodeReservationError::Store)?;
        if previous.as_ref().is_some_and(|record| {
            record.consumed_at.is_none() && record.resend_after_unix > now_unix
        }) {
            return Err(RegistrationEmailCodeReservationError::ResendTooSoon);
        }
        let (window_start_unix, send_count) = match previous.as_ref() {
            Some(record) if now_unix - record.window_start_unix < 3600 => {
                if record.send_count >= hourly_limit {
                    return Err(RegistrationEmailCodeReservationError::HourlyLimitReached);
                }
                (record.window_start_unix, record.send_count + 1)
            }
            _ => (now_unix, 1),
        };
        let record = RegistrationEmailCodeRecord {
            email: email.to_string(),
            code_hash,
            invite_code_hash,
            expires_at_unix: now_unix + ttl_seconds,
            resend_after_unix: now_unix + resend_seconds,
            attempts: 0,
            send_count,
            window_start_unix,
            consumed_at: None,
            created_at: previous
                .as_ref()
                .map(|record| record.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        sqlx::query(r#"INSERT INTO registration_email_codes (email,expires_at,consumed_at,updated_at,data)
            VALUES ($1,$2,NULL,$3,$4) ON CONFLICT (email) DO UPDATE SET expires_at=EXCLUDED.expires_at,
            consumed_at=NULL,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data"#)
            .bind(&record.email)
            .bind(record.expires_at_unix)
            .bind(timestamp(&record.updated_at).map_err(RegistrationEmailCodeReservationError::Store)?)
            .bind(json(&record).map_err(RegistrationEmailCodeReservationError::Store)?)
            .execute(&mut *tx)
            .await
            .map_err(|error| RegistrationEmailCodeReservationError::Store(db_error(error)))?;
        tx.commit()
            .await
            .map_err(|error| RegistrationEmailCodeReservationError::Store(db_error(error)))?;
        Ok(RegistrationEmailCodeReservation { record, previous })
    }

    pub async fn restore_registration_email_code_reservation(
        &self,
        reservation: &RegistrationEmailCodeReservation,
    ) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(&reservation.record.email)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        let current_hash = sqlx::query_scalar::<_, String>(
            "SELECT data->>'code_hash' FROM registration_email_codes WHERE email=$1 FOR UPDATE",
        )
        .bind(&reservation.record.email)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        if current_hash.as_deref() != Some(reservation.record.code_hash.as_str()) {
            tx.commit().await.map_err(db_error)?;
            return Ok(());
        }
        if let Some(previous) = reservation.previous.as_ref() {
            sqlx::query(
                r#"UPDATE registration_email_codes SET expires_at=$2,consumed_at=$3,
                updated_at=$4,data=$5 WHERE email=$1"#,
            )
            .bind(&previous.email)
            .bind(previous.expires_at_unix)
            .bind(optional_timestamp(previous.consumed_at.as_deref())?)
            .bind(timestamp(&previous.updated_at)?)
            .bind(json(previous)?)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        } else {
            sqlx::query("DELETE FROM registration_email_codes WHERE email=$1")
                .bind(&reservation.record.email)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
        }
        tx.commit().await.map_err(db_error)
    }

    pub async fn verify_registration_email_code_attempt(
        &self,
        email: &str,
        expected_code_hash: &str,
        invite_code_hash: &str,
        now_unix: i64,
        now: &str,
        max_attempts: i64,
    ) -> Result<bool, String> {
        let failed = sqlx::query_scalar::<_, i64>(
            r#"UPDATE registration_email_codes SET
            updated_at=$5,
            data=jsonb_set(jsonb_set(data,'{attempts}',to_jsonb((data->>'attempts')::bigint+1)),
                '{updated_at}',to_jsonb($6::text))
            WHERE email=$1 AND consumed_at IS NULL AND expires_at>=$2
              AND data->>'invite_code_hash'=$3
              AND (data->>'attempts')::bigint<$4
              AND data->>'code_hash'<>$7
            RETURNING (data->>'attempts')::bigint"#,
        )
        .bind(email)
        .bind(now_unix)
        .bind(invite_code_hash)
        .bind(max_attempts)
        .bind(timestamp(now)?)
        .bind(now)
        .bind(expected_code_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?;
        if failed.is_some() {
            return Ok(false);
        }
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS(
            SELECT 1 FROM registration_email_codes
            WHERE email=$1 AND consumed_at IS NULL AND expires_at>=$2
              AND data->>'invite_code_hash'=$3
              AND (data->>'attempts')::bigint<$4
              AND data->>'code_hash'=$5)"#,
        )
        .bind(email)
        .bind(now_unix)
        .bind(invite_code_hash)
        .bind(max_attempts)
        .bind(expected_code_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)
    }

    pub async fn mark_registration_email_code_consumed(&self, email: &str) -> Result<(), String> {
        let now = super::now_rfc3339();
        sqlx::query(r#"UPDATE registration_email_codes SET consumed_at=$2,updated_at=$2,
            data=jsonb_set(jsonb_set(data,'{consumed_at}',to_jsonb($3::text)),'{updated_at}',to_jsonb($3::text)) WHERE email=$1"#)
            .bind(email).bind(timestamp(&now)?).bind(&now).execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }
}
