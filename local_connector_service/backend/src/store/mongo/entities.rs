// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoConnectorStore {
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
                        "sandbox_readiness": &pairing.sandbox_readiness,
                        "permission_profile_id": &pairing.permission_profile_id,
                        "approval_policy": &pairing.approval_policy,
                        "approval_reviewer": &pairing.approval_reviewer,
                        "policy_revision": &pairing.policy_revision,
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
                        "sandbox_readiness": &pairing.sandbox_readiness,
                        "permission_profile_id": &pairing.permission_profile_id,
                        "approval_policy": &pairing.approval_policy,
                        "approval_reviewer": &pairing.approval_reviewer,
                        "policy_revision": &pairing.policy_revision,
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
