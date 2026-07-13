// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;
use mongodb::options::IndexOptions;
use mongodb::{Collection, IndexModel};

use super::MongoConnectorStore;

impl MongoConnectorStore {
    pub(super) async fn ensure_indexes(&self) -> Result<(), String> {
        ensure_mongo_index(&self.devices, doc! { "id": 1 }, true).await?;
        ensure_mongo_index(
            &self.devices,
            doc! { "owner_user_id": 1, "updated_at": -1 },
            false,
        )
        .await?;
        ensure_mongo_index(&self.devices, doc! { "status": 1 }, false).await?;

        ensure_mongo_index(&self.workspaces, doc! { "id": 1 }, true).await?;
        ensure_mongo_index(
            &self.workspaces,
            doc! { "owner_user_id": 1, "updated_at": -1 },
            false,
        )
        .await?;
        ensure_mongo_index(&self.workspaces, doc! { "device_id": 1 }, false).await?;

        ensure_mongo_index(&self.project_bindings, doc! { "id": 1 }, true).await?;
        ensure_mongo_index(
            &self.project_bindings,
            doc! { "owner_user_id": 1, "project_id": 1, "mode": 1 },
            true,
        )
        .await?;
        ensure_mongo_index(&self.project_bindings, doc! { "workspace_id": 1 }, false).await?;

        ensure_mongo_index(&self.sandbox_pairings, doc! { "id": 1 }, true).await?;
        ensure_mongo_index(
            &self.sandbox_pairings,
            doc! { "owner_user_id": 1, "device_id": 1, "workspace_id": 1 },
            true,
        )
        .await?;
        ensure_mongo_index(
            &self.sandbox_pairings,
            doc! { "owner_user_id": 1, "updated_at": -1 },
            false,
        )
        .await?;
        ensure_mongo_index(&self.sandbox_pairings, doc! { "workspace_id": 1 }, false).await?;

        ensure_mongo_index(&self.sessions, doc! { "id": 1 }, true).await?;
        ensure_mongo_index(&self.sessions, doc! { "owner_user_id": 1 }, true).await?;
        ensure_mongo_index(&self.sessions, doc! { "device_id": 1, "status": 1 }, false).await?;
        ensure_mongo_index(&self.sessions, doc! { "expires_at": 1 }, false).await?;
        Ok(())
    }
}

async fn ensure_mongo_index<T>(
    collection: &Collection<T>,
    keys: mongodb::bson::Document,
    unique: bool,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder().unique(unique).build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model, None)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}
