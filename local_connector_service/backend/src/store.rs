// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    LocalConnectorDevice, LocalConnectorProjectBinding, LocalConnectorSandboxPairing,
    LocalConnectorSession, LocalConnectorWorkspace,
};

mod mongo;

pub use self::mongo::MongoConnectorStore;

#[derive(Debug)]
pub enum SessionAcquireError {
    AlreadyActive,
    Store(String),
}

#[derive(Clone)]
pub enum ConnectorStore {
    Mongo(MongoConnectorStore),
}

impl ConnectorStore {
    pub async fn connect(database_url: &str) -> Result<Self, String> {
        let normalized = database_url.trim();
        if normalized.starts_with("mongodb://") || normalized.starts_with("mongodb+srv://") {
            return MongoConnectorStore::connect(normalized)
                .await
                .map(Self::Mongo);
        }
        Err(format!(
            "unsupported LOCAL_CONNECTOR_DATABASE_URL; expected mongodb:// or mongodb+srv://, got: {normalized}"
        ))
    }

    pub async fn create_device(&self, device: &LocalConnectorDevice) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.create_device(device).await,
        }
    }

    pub async fn get_device(&self, id: &str) -> Result<Option<LocalConnectorDevice>, String> {
        match self {
            Self::Mongo(store) => store.get_device(id).await,
        }
    }

    pub async fn list_devices(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalConnectorDevice>, String> {
        match self {
            Self::Mongo(store) => {
                store.cleanup_expired_owner_session(owner_user_id).await?;
                store.list_devices(owner_user_id).await
            }
        }
    }

    pub async fn mark_device_online(&self, id: &str) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.mark_device_online(id).await,
        }
    }

    pub async fn mark_device_offline(&self, id: &str) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.mark_device_offline(id).await,
        }
    }

    pub async fn revoke_device(&self, owner_user_id: &str, id: &str) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.revoke_device(owner_user_id, id).await,
        }
    }

    pub async fn create_workspace(
        &self,
        workspace: &LocalConnectorWorkspace,
    ) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.create_workspace(workspace).await,
        }
    }

    pub async fn get_workspace(&self, id: &str) -> Result<Option<LocalConnectorWorkspace>, String> {
        match self {
            Self::Mongo(store) => store.get_workspace(id).await,
        }
    }

    pub async fn list_workspaces(
        &self,
        owner_user_id: &str,
        device_id: Option<String>,
    ) -> Result<Vec<LocalConnectorWorkspace>, String> {
        match self {
            Self::Mongo(store) => store.list_workspaces(owner_user_id, device_id).await,
        }
    }

    pub async fn update_workspace(
        &self,
        workspace: &LocalConnectorWorkspace,
    ) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.update_workspace(workspace).await,
        }
    }

    pub async fn delete_workspace(&self, owner_user_id: &str, id: &str) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.delete_workspace(owner_user_id, id).await,
        }
    }

    pub async fn upsert_project_binding(
        &self,
        binding: &LocalConnectorProjectBinding,
    ) -> Result<LocalConnectorProjectBinding, String> {
        match self {
            Self::Mongo(store) => store.upsert_project_binding(binding).await,
        }
    }

    pub async fn get_project_binding(
        &self,
        id: &str,
    ) -> Result<Option<LocalConnectorProjectBinding>, String> {
        match self {
            Self::Mongo(store) => store.get_project_binding(id).await,
        }
    }

    pub async fn list_project_bindings(
        &self,
        owner_user_id: &str,
        project_id: Option<String>,
        mode: Option<String>,
    ) -> Result<Vec<LocalConnectorProjectBinding>, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .list_project_bindings(owner_user_id, project_id, mode)
                    .await
            }
        }
    }

    pub async fn update_project_binding(
        &self,
        binding: &LocalConnectorProjectBinding,
    ) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.update_project_binding(binding).await,
        }
    }

    pub async fn delete_project_binding(
        &self,
        owner_user_id: &str,
        id: &str,
    ) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.delete_project_binding(owner_user_id, id).await,
        }
    }

    pub async fn upsert_sandbox_pairing(
        &self,
        pairing: &LocalConnectorSandboxPairing,
    ) -> Result<LocalConnectorSandboxPairing, String> {
        match self {
            Self::Mongo(store) => store.upsert_sandbox_pairing(pairing).await,
        }
    }

    pub async fn get_sandbox_pairing(
        &self,
        id: &str,
    ) -> Result<Option<LocalConnectorSandboxPairing>, String> {
        match self {
            Self::Mongo(store) => store.get_sandbox_pairing(id).await,
        }
    }

    pub async fn list_sandbox_pairings(
        &self,
        owner_user_id: &str,
        device_id: Option<String>,
        workspace_id: Option<String>,
    ) -> Result<Vec<LocalConnectorSandboxPairing>, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .list_sandbox_pairings(owner_user_id, device_id, workspace_id)
                    .await
            }
        }
    }

    pub async fn update_sandbox_pairing(
        &self,
        pairing: &LocalConnectorSandboxPairing,
    ) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.update_sandbox_pairing(pairing).await,
        }
    }

    pub async fn delete_sandbox_pairing(
        &self,
        owner_user_id: &str,
        id: &str,
    ) -> Result<(), String> {
        match self {
            Self::Mongo(store) => store.delete_sandbox_pairing(owner_user_id, id).await,
        }
    }

    pub async fn open_session(
        &self,
        session: &LocalConnectorSession,
    ) -> Result<(), SessionAcquireError> {
        match self {
            Self::Mongo(store) => store.open_session(session).await,
        }
    }

    pub async fn heartbeat_session(
        &self,
        owner_user_id: &str,
        session_id: &str,
        device_id: &str,
        lease_ttl: std::time::Duration,
    ) -> Result<bool, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .heartbeat_session(owner_user_id, session_id, device_id, lease_ttl)
                    .await
            }
        }
    }

    pub async fn close_session(
        &self,
        owner_user_id: &str,
        session_id: &str,
        device_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .close_session(owner_user_id, session_id, device_id)
                    .await
            }
        }
    }

    pub async fn close_device_session(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::Mongo(store) => store.close_device_session(owner_user_id, device_id).await,
        }
    }

    pub async fn session_holds_active_lease(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::Mongo(store) => {
                store
                    .session_holds_active_lease(owner_user_id, device_id)
                    .await
            }
        }
    }

    pub async fn active_session(
        &self,
        owner_user_id: &str,
    ) -> Result<Option<LocalConnectorSession>, String> {
        match self {
            Self::Mongo(store) => store.active_session(owner_user_id).await,
        }
    }
}
