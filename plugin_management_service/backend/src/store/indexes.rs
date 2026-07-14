// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;
use mongodb::options::IndexOptions;
use mongodb::{Collection, IndexModel};

use super::AppStore;

impl AppStore {
    pub async fn initialize(&self) -> Result<(), String> {
        create_unique_index(&self.mcps, doc! { "id": 1 }).await?;
        create_index(
            &self.mcps,
            doc! { "owner_user_id": 1, "visibility": 1, "enabled": 1 },
        )
        .await?;
        create_index(&self.mcps, doc! { "visibility": 1, "enabled": 1 }).await?;
        create_index(&self.mcps, doc! { "runtime.kind": 1 }).await?;
        create_index(
            &self.mcps,
            doc! { "runtime.local_connector.device_id": 1, "runtime.local_connector.workspace_id": 1 },
        )
        .await?;
        create_local_manifest_unique_index(
            &self.mcps,
            doc! {
                "owner_user_id": 1,
                "runtime.local_connector.device_id": 1,
                "runtime.local_connector.manifest_id": 1,
            },
        )
        .await?;

        create_unique_index(&self.skills, doc! { "id": 1 }).await?;
        create_index(
            &self.skills,
            doc! { "owner_user_id": 1, "visibility": 1, "enabled": 1 },
        )
        .await?;
        create_index(&self.skills, doc! { "visibility": 1, "enabled": 1 }).await?;
        create_index(&self.skills, doc! { "content.kind": 1 }).await?;

        create_unique_index(&self.skill_packages, doc! { "id": 1 }).await?;
        create_index(
            &self.skill_packages,
            doc! { "owner_user_id": 1, "visibility": 1 },
        )
        .await?;

        create_unique_index(&self.agents, doc! { "agent_key": 1 }).await?;
        create_index(&self.agents, doc! { "service_name": 1, "enabled": 1 }).await?;

        create_unique_index(&self.bindings, doc! { "id": 1 }).await?;
        create_index(
            &self.bindings,
            doc! { "agent_key": 1, "binding_scope": 1, "owner_user_id": 1 },
        )
        .await?;
        create_index(
            &self.bindings,
            doc! { "resource_kind": 1, "resource_id": 1 },
        )
        .await?;

        create_unique_index(&self.checks, doc! { "id": 1 }).await?;
        create_index(&self.checks, doc! { "resource_kind": 1, "resource_id": 1 }).await?;
        create_unique_index(
            &self.skill_preferences,
            doc! { "owner_user_id": 1, "skill_id": 1 },
        )
        .await?;
        create_index(
            &self.skill_preferences,
            doc! { "owner_user_id": 1, "enabled": 1 },
        )
        .await?;
        create_unique_index(
            &self.skill_installations,
            doc! { "owner_user_id": 1, "device_id": 1, "skill_id": 1 },
        )
        .await?;
        create_index(
            &self.skill_installations,
            doc! { "owner_user_id": 1, "skill_id": 1, "status": 1 },
        )
        .await?;
        Ok(())
    }
}

async fn create_index<T>(
    collection: &Collection<T>,
    keys: mongodb::bson::Document,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let model = IndexModel::builder().keys(keys).build();
    collection
        .create_index(model, None)
        .await
        .map_err(|err| format!("create mongodb index failed: {err}"))?;
    Ok(())
}

async fn create_unique_index<T>(
    collection: &Collection<T>,
    keys: mongodb::bson::Document,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder().unique(true).build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model, None)
        .await
        .map_err(|err| format!("create mongodb unique index failed: {err}"))?;
    Ok(())
}

async fn create_local_manifest_unique_index<T>(
    collection: &Collection<T>,
    keys: mongodb::bson::Document,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder()
        .unique(true)
        .partial_filter_expression(doc! {
            "runtime.local_connector.manifest_id": { "$type": "string" }
        })
        .build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model, None)
        .await
        .map_err(|err| format!("create local manifest MongoDB unique index failed: {err}"))?;
    Ok(())
}
