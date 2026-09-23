// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    lease_deadline_rfc3339, lease_now_rfc3339, now_rfc3339, LocalConnectorSession,
    DEVICE_STATUS_OFFLINE, DEVICE_STATUS_REVOKED, SESSION_STATUS_CONNECTED,
};

use super::super::{
    db_error, decode_optional, json, timestamp, ConnectorStore, SessionAcquireError,
};

impl ConnectorStore {
    pub async fn open_session(
        &self,
        session: &LocalConnectorSession,
    ) -> Result<(), SessionAcquireError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| SessionAcquireError::Store(err.to_string()))?;
        let acquired = sqlx::query_scalar::<_, String>(
            "INSERT INTO local_connector_active_sessions(id,owner_user_id,device_id,status,expires_at,updated_at,data)
             VALUES($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT(owner_user_id) DO UPDATE SET id=EXCLUDED.id,device_id=EXCLUDED.device_id,
               status=EXCLUDED.status,expires_at=EXCLUDED.expires_at,updated_at=EXCLUDED.updated_at,
               data=EXCLUDED.data
             WHERE local_connector_active_sessions.expires_at<=now()
               OR local_connector_active_sessions.status<>$8
             RETURNING id",
        )
        .bind(&session.id)
        .bind(&session.owner_user_id)
        .bind(&session.device_id)
        .bind(&session.status)
        .bind(timestamp(&session.expires_at).map_err(SessionAcquireError::Store)?)
        .bind(timestamp(&session.updated_at).map_err(SessionAcquireError::Store)?)
        .bind(json(session).map_err(SessionAcquireError::Store)?)
        .bind(SESSION_STATUS_CONNECTED)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|err| SessionAcquireError::Store(err.to_string()))?;
        if acquired.is_none() {
            return Err(SessionAcquireError::AlreadyActive);
        }
        let now = now_rfc3339();
        sqlx::query(
            "UPDATE local_connector_devices SET status=$3,updated_at=$4,
             data=data || jsonb_build_object('status',$3::text,'updated_at',$5::text)
             WHERE owner_user_id=$1 AND id<>$2 AND status NOT IN ($6,$3)",
        )
        .bind(&session.owner_user_id)
        .bind(&session.device_id)
        .bind(DEVICE_STATUS_OFFLINE)
        .bind(timestamp(&now).map_err(SessionAcquireError::Store)?)
        .bind(now)
        .bind(DEVICE_STATUS_REVOKED)
        .execute(&mut *tx)
        .await
        .map_err(|err| SessionAcquireError::Store(err.to_string()))?;
        tx.commit()
            .await
            .map_err(|err| SessionAcquireError::Store(err.to_string()))
    }

    pub async fn heartbeat_session(
        &self,
        owner_user_id: &str,
        session_id: &str,
        device_id: &str,
        lease_ttl: std::time::Duration,
    ) -> Result<bool, String> {
        let now = lease_now_rfc3339();
        let expires_at = lease_deadline_rfc3339(lease_ttl);
        let result = sqlx::query(
            "UPDATE local_connector_active_sessions SET expires_at=$5,updated_at=$4,
             data=data || jsonb_build_object('last_heartbeat_at',$6::text,'expires_at',$7::text,'updated_at',$6::text)
             WHERE owner_user_id=$1 AND id=$2 AND device_id=$3 AND status=$8 AND expires_at>now()",
        ).bind(owner_user_id).bind(session_id).bind(device_id).bind(timestamp(&now)?)
            .bind(timestamp(&expires_at)?).bind(&now).bind(&expires_at).bind(SESSION_STATUS_CONNECTED)
            .execute(&self.pool).await.map_err(db_error)?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        self.mark_device_online(device_id).await?;
        Ok(true)
    }

    pub async fn close_session(
        &self,
        owner_user_id: &str,
        session_id: &str,
        device_id: &str,
    ) -> Result<bool, String> {
        let result = sqlx::query("DELETE FROM local_connector_active_sessions WHERE owner_user_id=$1 AND id=$2 AND device_id=$3")
            .bind(owner_user_id).bind(session_id).bind(device_id).execute(&self.pool).await.map_err(db_error)?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        self.mark_device_offline(device_id).await?;
        Ok(true)
    }

    pub async fn close_device_session(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<bool, String> {
        let result = sqlx::query(
            "DELETE FROM local_connector_active_sessions WHERE owner_user_id=$1 AND device_id=$2",
        )
        .bind(owner_user_id)
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(db_error)?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        self.mark_device_offline(device_id).await?;
        Ok(true)
    }

    pub async fn session_holds_active_lease(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<bool, String> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM local_connector_active_sessions WHERE owner_user_id=$1 AND device_id=$2 AND status=$3 AND expires_at>now())",
        ).bind(owner_user_id).bind(device_id).bind(SESSION_STATUS_CONNECTED)
            .fetch_one(&self.pool).await.map_err(db_error)
    }

    pub async fn active_session(
        &self,
        owner_user_id: &str,
    ) -> Result<Option<LocalConnectorSession>, String> {
        decode_optional(sqlx::query_scalar(
            "SELECT data FROM local_connector_active_sessions WHERE owner_user_id=$1 AND status=$2 AND expires_at>now()",
        ).bind(owner_user_id).bind(SESSION_STATUS_CONNECTED).fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub(super) async fn cleanup_expired_owner_session(
        &self,
        owner_user_id: &str,
    ) -> Result<(), String> {
        let device_id = sqlx::query_scalar::<_, String>(
            "DELETE FROM local_connector_active_sessions WHERE owner_user_id=$1 AND expires_at<=now() RETURNING device_id",
        ).bind(owner_user_id).fetch_optional(&self.pool).await.map_err(db_error)?;
        if let Some(device_id) = device_id {
            self.mark_device_offline(&device_id).await?;
        }
        Ok(())
    }
}
