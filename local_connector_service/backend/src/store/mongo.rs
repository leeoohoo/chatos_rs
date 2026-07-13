// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    lease_deadline_rfc3339, lease_now_rfc3339, now_rfc3339, LocalConnectorDevice,
    LocalConnectorProjectBinding, LocalConnectorSandboxPairing, LocalConnectorSession,
    LocalConnectorWorkspace, DEVICE_STATUS_OFFLINE, DEVICE_STATUS_ONLINE, DEVICE_STATUS_REVOKED,
    SESSION_STATUS_CONNECTED,
};
use crate::store::SessionAcquireError;
use futures::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use mongodb::{Client, Collection};

mod indexes;

#[derive(Clone)]
pub struct MongoConnectorStore {
    devices: Collection<LocalConnectorDevice>,
    workspaces: Collection<LocalConnectorWorkspace>,
    project_bindings: Collection<LocalConnectorProjectBinding>,
    sandbox_pairings: Collection<LocalConnectorSandboxPairing>,
    sessions: Collection<LocalConnectorSession>,
}

impl MongoConnectorStore {
    pub async fn connect(database_url: &str) -> Result<Self, String> {
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|err| format!("connect local connector mongodb failed: {err}"))?;
        let database = client.default_database().ok_or_else(|| {
            "LOCAL_CONNECTOR_DATABASE_URL mongodb connection string must include a database name"
                .to_string()
        })?;
        let store = Self {
            devices: database.collection("local_connector_devices"),
            workspaces: database.collection("local_connector_workspaces"),
            project_bindings: database.collection("local_connector_project_bindings"),
            sandbox_pairings: database.collection("local_connector_sandbox_pairings"),
            sessions: database.collection("local_connector_active_sessions"),
        };
        store.ensure_indexes().await?;
        Ok(store)
    }

    pub async fn create_device(&self, device: &LocalConnectorDevice) -> Result<(), String> {
        self.devices
            .insert_one(device, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_device(&self, id: &str) -> Result<Option<LocalConnectorDevice>, String> {
        self.devices
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_devices(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalConnectorDevice>, String> {
        self.find_devices(doc! { "owner_user_id": owner_user_id })
            .await
    }

    pub async fn mark_device_online(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        self.devices
            .update_one(
                doc! { "id": id, "status": { "$ne": DEVICE_STATUS_REVOKED } },
                doc! { "$set": { "status": DEVICE_STATUS_ONLINE, "last_seen_at": &now, "updated_at": &now } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn mark_device_offline(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        self.devices
            .update_one(
                doc! { "id": id, "status": { "$ne": DEVICE_STATUS_REVOKED } },
                doc! { "$set": { "status": DEVICE_STATUS_OFFLINE, "updated_at": &now } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn revoke_device(&self, owner_user_id: &str, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        self.devices
            .update_one(
                doc! { "id": id, "owner_user_id": owner_user_id },
                doc! { "$set": { "status": DEVICE_STATUS_REVOKED, "revoked_at": &now, "updated_at": &now } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn create_workspace(
        &self,
        workspace: &LocalConnectorWorkspace,
    ) -> Result<(), String> {
        self.workspaces
            .insert_one(workspace, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_workspace(&self, id: &str) -> Result<Option<LocalConnectorWorkspace>, String> {
        self.workspaces
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_workspaces(
        &self,
        owner_user_id: &str,
        device_id: Option<String>,
    ) -> Result<Vec<LocalConnectorWorkspace>, String> {
        let mut filter = doc! { "owner_user_id": owner_user_id };
        if let Some(device_id) = device_id {
            filter.insert("device_id", device_id);
        }
        self.find_workspaces(filter).await
    }

    pub async fn update_workspace(
        &self,
        workspace: &LocalConnectorWorkspace,
    ) -> Result<(), String> {
        let now = now_rfc3339();
        self.workspaces
            .update_one(
                doc! { "id": &workspace.id, "owner_user_id": &workspace.owner_user_id },
                doc! {
                    "$set": {
                        "device_id": &workspace.device_id,
                        "display_name": &workspace.display_name,
                        "local_path_alias": &workspace.local_path_alias,
                        "local_path_fingerprint": &workspace.local_path_fingerprint,
                        "capabilities": &workspace.capabilities,
                        "status": &workspace.status,
                        "updated_at": &now,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_workspace(&self, owner_user_id: &str, id: &str) -> Result<(), String> {
        self.workspaces
            .delete_one(doc! { "id": id, "owner_user_id": owner_user_id }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn upsert_project_binding(
        &self,
        binding: &LocalConnectorProjectBinding,
    ) -> Result<LocalConnectorProjectBinding, String> {
        let now = now_rfc3339();
        let options = FindOneAndUpdateOptions::builder()
            .upsert(true)
            .return_document(ReturnDocument::After)
            .build();
        self.project_bindings
            .find_one_and_update(
                doc! {
                    "owner_user_id": &binding.owner_user_id,
                    "project_id": &binding.project_id,
                    "mode": &binding.mode,
                },
                doc! {
                    "$setOnInsert": {
                        "id": &binding.id,
                        "created_at": &binding.created_at,
                    },
                    "$set": {
                        "owner_user_id": &binding.owner_user_id,
                        "project_id": &binding.project_id,
                        "mode": &binding.mode,
                        "device_id": &binding.device_id,
                        "workspace_id": &binding.workspace_id,
                        "enabled": binding.enabled,
                        "updated_at": &now,
                    }
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "project binding not found after upsert".to_string())
    }

    pub async fn get_project_binding(
        &self,
        id: &str,
    ) -> Result<Option<LocalConnectorProjectBinding>, String> {
        self.project_bindings
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_project_bindings(
        &self,
        owner_user_id: &str,
        project_id: Option<String>,
        mode: Option<String>,
    ) -> Result<Vec<LocalConnectorProjectBinding>, String> {
        let mut filter = doc! { "owner_user_id": owner_user_id };
        if let Some(project_id) = project_id {
            filter.insert("project_id", project_id);
        }
        if let Some(mode) = mode {
            filter.insert("mode", mode);
        }
        self.find_project_bindings(filter).await
    }

    pub async fn update_project_binding(
        &self,
        binding: &LocalConnectorProjectBinding,
    ) -> Result<(), String> {
        let now = now_rfc3339();
        self.project_bindings
            .update_one(
                doc! { "id": &binding.id, "owner_user_id": &binding.owner_user_id },
                doc! {
                    "$set": {
                        "device_id": &binding.device_id,
                        "workspace_id": &binding.workspace_id,
                        "enabled": binding.enabled,
                        "updated_at": &now,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_project_binding(
        &self,
        owner_user_id: &str,
        id: &str,
    ) -> Result<(), String> {
        self.project_bindings
            .delete_one(doc! { "id": id, "owner_user_id": owner_user_id }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn upsert_sandbox_pairing(
        &self,
        pairing: &LocalConnectorSandboxPairing,
    ) -> Result<LocalConnectorSandboxPairing, String> {
        let now = now_rfc3339();
        let options = FindOneAndUpdateOptions::builder()
            .upsert(true)
            .return_document(ReturnDocument::After)
            .build();
        self.sandbox_pairings
            .find_one_and_update(
                doc! {
                    "owner_user_id": &pairing.owner_user_id,
                    "device_id": &pairing.device_id,
                    "workspace_id": &pairing.workspace_id,
                },
                doc! {
                    "$setOnInsert": {
                        "id": &pairing.id,
                        "created_at": &pairing.created_at,
                    },
                    "$set": {
                        "owner_user_id": &pairing.owner_user_id,
                        "device_id": &pairing.device_id,
                        "workspace_id": &pairing.workspace_id,
                        "enabled": pairing.enabled,
                        "sandbox_mode": &pairing.sandbox_mode,
                        "facade_base_url": &pairing.facade_base_url,
                        "access_client_id": &pairing.access_client_id,
                        "updated_at": &now,
                    }
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "sandbox pairing not found after upsert".to_string())
    }

    pub async fn get_sandbox_pairing(
        &self,
        id: &str,
    ) -> Result<Option<LocalConnectorSandboxPairing>, String> {
        self.sandbox_pairings
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_sandbox_pairings(
        &self,
        owner_user_id: &str,
        device_id: Option<String>,
        workspace_id: Option<String>,
    ) -> Result<Vec<LocalConnectorSandboxPairing>, String> {
        let mut filter = doc! { "owner_user_id": owner_user_id };
        if let Some(device_id) = device_id {
            filter.insert("device_id", device_id);
        }
        if let Some(workspace_id) = workspace_id {
            filter.insert("workspace_id", workspace_id);
        }
        self.find_sandbox_pairings(filter).await
    }

    pub async fn update_sandbox_pairing(
        &self,
        pairing: &LocalConnectorSandboxPairing,
    ) -> Result<(), String> {
        let now = now_rfc3339();
        self.sandbox_pairings
            .update_one(
                doc! { "id": &pairing.id, "owner_user_id": &pairing.owner_user_id },
                doc! {
                    "$set": {
                        "workspace_id": &pairing.workspace_id,
                        "enabled": pairing.enabled,
                        "sandbox_mode": &pairing.sandbox_mode,
                        "facade_base_url": &pairing.facade_base_url,
                        "access_client_id": &pairing.access_client_id,
                        "updated_at": &now,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_sandbox_pairing(
        &self,
        owner_user_id: &str,
        id: &str,
    ) -> Result<(), String> {
        self.sandbox_pairings
            .delete_one(doc! { "id": id, "owner_user_id": owner_user_id }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

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

    async fn find_devices(
        &self,
        filter: mongodb::bson::Document,
    ) -> Result<Vec<LocalConnectorDevice>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1 })
            .build();
        let cursor = self
            .devices
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?;
        cursor.try_collect().await.map_err(|err| err.to_string())
    }

    async fn find_workspaces(
        &self,
        filter: mongodb::bson::Document,
    ) -> Result<Vec<LocalConnectorWorkspace>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1 })
            .build();
        let cursor = self
            .workspaces
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?;
        cursor.try_collect().await.map_err(|err| err.to_string())
    }

    async fn find_project_bindings(
        &self,
        filter: mongodb::bson::Document,
    ) -> Result<Vec<LocalConnectorProjectBinding>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1 })
            .build();
        let cursor = self
            .project_bindings
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?;
        cursor.try_collect().await.map_err(|err| err.to_string())
    }

    async fn find_sandbox_pairings(
        &self,
        filter: mongodb::bson::Document,
    ) -> Result<Vec<LocalConnectorSandboxPairing>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1 })
            .build();
        let cursor = self
            .sandbox_pairings
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?;
        cursor.try_collect().await.map_err(|err| err.to_string())
    }
}

fn is_duplicate_key_error(error: &mongodb::error::Error) -> bool {
    error.to_string().contains("E11000") || error.to_string().contains("duplicate key")
}
