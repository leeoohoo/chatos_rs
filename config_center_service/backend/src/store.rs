// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::{FindOptions, IndexOptions, ReplaceOptions};
use mongodb::{Collection, Database, IndexModel};

use chatos_config_sdk::ConfigSnapshot;

use crate::models::{
    ActiveReleaseRecord, AuditEventRecord, ConfigDefinitionRecord, ConfigDraftRecord,
    ConfigReleaseRecord, ServiceInstanceRecord,
};

#[derive(Clone)]
pub struct AppStore {
    database: Database,
    definitions: Collection<ConfigDefinitionRecord>,
    drafts: Collection<ConfigDraftRecord>,
    releases: Collection<ConfigReleaseRecord>,
    snapshots: Collection<ConfigSnapshot>,
    active_releases: Collection<ActiveReleaseRecord>,
    audit_events: Collection<AuditEventRecord>,
    instances: Collection<ServiceInstanceRecord>,
}

impl AppStore {
    pub fn new(database: Database) -> Self {
        Self {
            definitions: database.collection("config_definitions"),
            drafts: database.collection("config_drafts"),
            releases: database.collection("config_releases"),
            snapshots: database.collection("config_snapshots"),
            active_releases: database.collection("config_active_releases"),
            audit_events: database.collection("config_audit_events"),
            instances: database.collection("config_service_instances"),
            database,
        }
    }

    pub async fn initialize(&self) -> Result<(), String> {
        unique_index(&self.definitions, doc! { "key": 1 }).await?;
        unique_index(&self.drafts, doc! { "environment": 1 }).await?;
        unique_index(&self.releases, doc! { "environment": 1, "revision": 1 }).await?;
        unique_index(
            &self.snapshots,
            doc! { "environment": 1, "service_name": 1, "revision": 1 },
        )
        .await?;
        unique_index(&self.active_releases, doc! { "environment": 1 }).await?;
        index(&self.audit_events, doc! { "created_at": -1 }).await?;
        unique_index(
            &self.instances,
            doc! { "environment": 1, "service_name": 1, "service_id": 1 },
        )
        .await
    }

    pub async fn ping(&self) -> Result<(), String> {
        self.database
            .run_command(doc! { "ping": 1 }, None)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn upsert_definition(
        &self,
        definition: &ConfigDefinitionRecord,
    ) -> Result<(), String> {
        self.definitions
            .replace_one(
                doc! { "key": &definition.key },
                definition,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn delete_definitions(&self, keys: &[&str]) -> Result<(), String> {
        let keys = keys.iter().map(|key| key.to_string()).collect::<Vec<_>>();
        self.definitions
            .delete_many(doc! { "key": { "$in": keys } }, None)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn list_definitions(&self) -> Result<Vec<ConfigDefinitionRecord>, String> {
        self.definitions
            .find(
                doc! {},
                FindOptions::builder()
                    .sort(doc! { "ui_order": 1, "key": 1 })
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_active(
        &self,
        environment: &str,
    ) -> Result<Option<ActiveReleaseRecord>, String> {
        self.active_releases
            .find_one(doc! { "environment": environment }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_active_releases(&self) -> Result<Vec<ActiveReleaseRecord>, String> {
        self.active_releases
            .find(doc! {}, None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn set_active(&self, active: &ActiveReleaseRecord) -> Result<(), String> {
        self.active_releases
            .replace_one(
                doc! { "environment": &active.environment },
                active,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn get_release(&self, id: &str) -> Result<Option<ConfigReleaseRecord>, String> {
        self.releases
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_active_release(
        &self,
        environment: &str,
    ) -> Result<Option<ConfigReleaseRecord>, String> {
        let Some(active) = self.get_active(environment).await? else {
            return Ok(None);
        };
        self.get_release(active.release_id.as_str()).await
    }

    pub async fn insert_release(&self, release: &ConfigReleaseRecord) -> Result<(), String> {
        self.releases
            .insert_one(release, None)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn save_release(&self, release: &ConfigReleaseRecord) -> Result<(), String> {
        self.releases
            .replace_one(doc! { "id": &release.id }, release, None)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn list_releases(
        &self,
        environment: &str,
        limit: i64,
    ) -> Result<Vec<ConfigReleaseRecord>, String> {
        self.releases
            .find(
                doc! { "environment": environment },
                FindOptions::builder()
                    .sort(doc! { "revision": -1 })
                    .limit(limit.max(1))
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_all_releases(&self) -> Result<Vec<ConfigReleaseRecord>, String> {
        self.releases
            .find(doc! {}, None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn next_release_revision(&self, environment: &str) -> Result<i64, String> {
        Ok(self
            .list_releases(environment, 1)
            .await?
            .first()
            .map(|release| release.revision.saturating_add(1))
            .unwrap_or(1))
    }

    pub async fn insert_snapshot(&self, snapshot: &ConfigSnapshot) -> Result<(), String> {
        self.snapshots
            .insert_one(snapshot, None)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn list_all_snapshots(&self) -> Result<Vec<ConfigSnapshot>, String> {
        self.snapshots
            .find(doc! {}, None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn save_snapshot(&self, snapshot: &ConfigSnapshot) -> Result<(), String> {
        self.snapshots
            .replace_one(
                doc! {
                    "environment": &snapshot.environment,
                    "service_name": &snapshot.service_name,
                    "revision": snapshot.revision,
                },
                snapshot,
                None,
            )
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn get_snapshot(
        &self,
        environment: &str,
        service_name: &str,
        revision: i64,
    ) -> Result<Option<ConfigSnapshot>, String> {
        self.snapshots
            .find_one(
                doc! {
                    "environment": environment,
                    "service_name": service_name,
                    "revision": revision,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_active_snapshot(
        &self,
        environment: &str,
        service_name: &str,
    ) -> Result<Option<ConfigSnapshot>, String> {
        let Some(active) = self.get_active(environment).await? else {
            return Ok(None);
        };
        self.get_snapshot(environment, service_name, active.revision)
            .await
    }

    pub async fn get_draft(&self, environment: &str) -> Result<Option<ConfigDraftRecord>, String> {
        self.drafts
            .find_one(doc! { "environment": environment }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_drafts(&self) -> Result<Vec<ConfigDraftRecord>, String> {
        self.drafts
            .find(doc! {}, None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn save_draft(&self, draft: &ConfigDraftRecord) -> Result<(), String> {
        self.drafts
            .replace_one(
                doc! { "environment": &draft.environment },
                draft,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn delete_draft(&self, environment: &str) -> Result<(), String> {
        self.drafts
            .delete_one(doc! { "environment": environment }, None)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn insert_audit(&self, event: &AuditEventRecord) -> Result<(), String> {
        self.audit_events
            .insert_one(event, None)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn list_audit(&self, limit: i64) -> Result<Vec<AuditEventRecord>, String> {
        self.audit_events
            .find(
                doc! {},
                FindOptions::builder()
                    .sort(doc! { "created_at": -1 })
                    .limit(limit.max(1))
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_all_audit(&self) -> Result<Vec<AuditEventRecord>, String> {
        self.audit_events
            .find(doc! {}, None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn save_audit(&self, event: &AuditEventRecord) -> Result<(), String> {
        self.audit_events
            .replace_one(doc! { "id": &event.id }, event, None)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn upsert_instance(&self, instance: &ServiceInstanceRecord) -> Result<(), String> {
        self.instances
            .replace_one(
                doc! {
                    "environment": &instance.environment,
                    "service_name": &instance.service_name,
                    "service_id": &instance.service_id,
                },
                instance,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub async fn list_instances(&self) -> Result<Vec<ServiceInstanceRecord>, String> {
        self.instances
            .find(
                doc! {},
                FindOptions::builder()
                    .sort(doc! { "environment": 1, "service_name": 1, "service_id": 1 })
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }
}

async fn index<T>(collection: &Collection<T>, keys: mongodb::bson::Document) -> Result<(), String>
where
    T: Send + Sync,
{
    collection
        .create_index(IndexModel::builder().keys(keys).build(), None)
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}

async fn unique_index<T>(
    collection: &Collection<T>,
    keys: mongodb::bson::Document,
) -> Result<(), String>
where
    T: Send + Sync,
{
    collection
        .create_index(
            IndexModel::builder()
                .keys(keys)
                .options(IndexOptions::builder().unique(true).build())
                .build(),
            None,
        )
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}
