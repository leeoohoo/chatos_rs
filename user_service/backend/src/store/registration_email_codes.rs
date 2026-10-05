// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sqlx::types::Json;

use super::{db_error, fetch_optional, json, optional_timestamp, timestamp, AppStore};
use crate::models::RegistrationEmailCodeRecord;

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

impl AppStore {
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
