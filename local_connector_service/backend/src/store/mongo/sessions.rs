// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoConnectorStore {
    pub async fn open_session(
        &self,
        session: &LocalConnectorSession,
    ) -> Result<(), SessionAcquireError> {
        let now = lease_now_rfc3339();
        let options = FindOneAndUpdateOptions::builder()
            .upsert(true)
            .return_document(ReturnDocument::After)
            .build();
        let result = self
            .sessions
            .find_one_and_update(
                doc! {
                    "owner_user_id": &session.owner_user_id,
                    "$or": [
                        { "expires_at": { "$lte": &now } },
                        { "status": { "$ne": SESSION_STATUS_CONNECTED } }
                    ]
                },
                doc! {
                    "$set": {
                        "id": &session.id,
                        "owner_user_id": &session.owner_user_id,
                        "device_id": &session.device_id,
                        "connection_id": &session.connection_id,
                        "status": &session.status,
                        "connected_at": &session.connected_at,
                        "last_heartbeat_at": &session.last_heartbeat_at,
                        "expires_at": &session.expires_at,
                        "disconnected_at": &session.disconnected_at,
                        "created_at": &session.created_at,
                        "updated_at": &session.updated_at,
                    }
                },
                options,
            )
            .await;
        match result {
            Ok(Some(_)) => {
                self.mark_owner_devices_offline_except(
                    session.owner_user_id.as_str(),
                    session.device_id.as_str(),
                )
                .await
                .map_err(SessionAcquireError::Store)?;
                Ok(())
            }
            Ok(None) => Err(SessionAcquireError::AlreadyActive),
            Err(err) if is_duplicate_key_error(&err) => Err(SessionAcquireError::AlreadyActive),
            Err(err) => Err(SessionAcquireError::Store(err.to_string())),
        }
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
        let result = self.sessions
            .update_one(
                doc! {
                    "id": session_id,
                    "owner_user_id": owner_user_id,
                    "device_id": device_id,
                    "status": SESSION_STATUS_CONNECTED,
                    "expires_at": { "$gt": &now },
                },
                doc! { "$set": { "last_heartbeat_at": &now, "expires_at": &expires_at, "updated_at": &now } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if result.matched_count == 0 {
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
        let result = self
            .sessions
            .delete_one(
                doc! { "id": session_id, "owner_user_id": owner_user_id, "device_id": device_id },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if result.deleted_count == 0 {
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
        let result = self
            .sessions
            .delete_one(
                doc! { "owner_user_id": owner_user_id, "device_id": device_id },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if result.deleted_count == 0 {
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
        let now = lease_now_rfc3339();
        self.sessions
            .find_one(
                doc! {
                    "owner_user_id": owner_user_id,
                    "device_id": device_id,
                    "status": SESSION_STATUS_CONNECTED,
                    "expires_at": { "$gt": &now },
                },
                None,
            )
            .await
            .map(|item| item.is_some())
            .map_err(|err| err.to_string())
    }

    pub async fn active_session(
        &self,
        owner_user_id: &str,
    ) -> Result<Option<LocalConnectorSession>, String> {
        let now = lease_now_rfc3339();
        self.sessions
            .find_one(
                doc! {
                    "owner_user_id": owner_user_id,
                    "status": SESSION_STATUS_CONNECTED,
                    "expires_at": { "$gt": &now },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn cleanup_expired_owner_session(&self, owner_user_id: &str) -> Result<(), String> {
        let now = lease_now_rfc3339();
        let Some(session) = self
            .sessions
            .find_one(
                doc! { "owner_user_id": owner_user_id, "expires_at": { "$lte": &now } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(());
        };
        let result = self
            .sessions
            .delete_one(
                doc! { "id": &session.id, "expires_at": &session.expires_at },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if result.deleted_count == 1 {
            self.mark_device_offline(session.device_id.as_str()).await?;
        }
        Ok(())
    }
    async fn mark_owner_devices_offline_except(
        &self,
        owner_user_id: &str,
        active_device_id: &str,
    ) -> Result<(), String> {
        let now = now_rfc3339();
        self.devices
            .update_many(
                doc! {
                    "owner_user_id": owner_user_id,
                    "id": { "$ne": active_device_id },
                    "status": { "$nin": [DEVICE_STATUS_REVOKED, DEVICE_STATUS_OFFLINE] },
                },
                doc! { "$set": { "status": DEVICE_STATUS_OFFLINE, "updated_at": &now } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }
}

fn is_duplicate_key_error(error: &mongodb::error::Error) -> bool {
    error.to_string().contains("E11000") || error.to_string().contains("duplicate key")
}
