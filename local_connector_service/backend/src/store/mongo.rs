// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use crate::models::{
    lease_deadline_rfc3339, lease_now_rfc3339, now_rfc3339, ApplicableManagedRequirementsLayer,
    LocalConnectorDevice, LocalConnectorProjectBinding, LocalConnectorSandboxPairing,
    LocalConnectorSession, LocalConnectorWorkspace, ManagedRequirementsAssignment,
    ManagedRequirementsPolicy, DEVICE_STATUS_OFFLINE, DEVICE_STATUS_ONLINE, DEVICE_STATUS_REVOKED,
    MANAGED_REQUIREMENTS_SCOPE_GLOBAL, MANAGED_REQUIREMENTS_SCOPE_ROLE,
    MANAGED_REQUIREMENTS_SCOPE_USER, SESSION_STATUS_CONNECTED,
};
use crate::store::SessionAcquireError;
use futures::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use mongodb::{Client, Collection};

mod indexes;

#[derive(Clone)]
pub struct MongoConnectorStore {
    pub(super) devices: Collection<LocalConnectorDevice>,
    pub(super) workspaces: Collection<LocalConnectorWorkspace>,
    pub(super) project_bindings: Collection<LocalConnectorProjectBinding>,
    pub(super) sandbox_pairings: Collection<LocalConnectorSandboxPairing>,
    pub(super) sessions: Collection<LocalConnectorSession>,
    pub(super) managed_requirements_policies: Collection<ManagedRequirementsPolicy>,
    pub(super) managed_requirements_assignments: Collection<ManagedRequirementsAssignment>,
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
            managed_requirements_policies: database
                .collection("local_connector_managed_requirements_policies"),
            managed_requirements_assignments: database
                .collection("local_connector_managed_requirements_assignments"),
        };
        store.ensure_indexes().await?;
        Ok(store)
    }
}

mod entities;
mod managed_requirements;
mod sessions;
#[cfg(test)]
mod tests;
