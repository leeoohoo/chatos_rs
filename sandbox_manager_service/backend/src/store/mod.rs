// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::{ClientOptions, FindOptions, IndexOptions, ReplaceOptions};
use mongodb::{Client, Collection, IndexModel};

use crate::models::{ListSandboxQuery, SandboxEventRecord, SandboxLeaseRecord, SandboxStatus};

#[derive(Clone)]
pub struct SandboxStore {
    leases: Collection<SandboxLeaseRecord>,
    events: Collection<SandboxEventRecord>,
}

impl SandboxStore {
    pub async fn new(database_url: &str, database_name: &str) -> Result<Self, String> {
        let options = ClientOptions::parse(database_url)
            .await
            .map_err(|err| format!("parse mongodb url failed: {err}"))?;
        let client = Client::with_options(options)
            .map_err(|err| format!("create mongodb client failed: {err}"))?;
        let db = client.database(database_name);
        let store = Self {
            leases: db.collection("sandbox_leases"),
            events: db.collection("sandbox_events"),
        };
        store.ensure_indexes().await?;
        Ok(store)
    }

    async fn ensure_indexes(&self) -> Result<(), String> {
        create_index(
            &self.leases,
            doc! { "sandbox_id": 1 },
            Some("idx_sandbox_leases_sandbox_id"),
            true,
        )
        .await?;
        create_index(
            &self.leases,
            doc! { "tenant_id": 1 },
            Some("idx_sandbox_leases_tenant"),
            false,
        )
        .await?;
        create_index(
            &self.leases,
            doc! { "project_id": 1 },
            Some("idx_sandbox_leases_project"),
            false,
        )
        .await?;
        create_index(
            &self.leases,
            doc! { "run_id": 1 },
            Some("idx_sandbox_leases_run"),
            false,
        )
        .await?;
        create_index(
            &self.leases,
            doc! { "status": 1, "expires_at": 1 },
            Some("idx_sandbox_leases_status_expires"),
            false,
        )
        .await?;
        create_index(
            &self.events,
            doc! { "sandbox_id": 1, "created_at": 1 },
            Some("idx_sandbox_events_sandbox_created"),
            false,
        )
        .await?;
        Ok(())
    }

    pub async fn create_lease(&self, record: &SandboxLeaseRecord) -> Result<(), String> {
        self.leases
            .insert_one(record, None)
            .await
            .map(|_| ())
            .map_err(|err| format!("insert sandbox lease failed: {err}"))
    }

    pub async fn replace_lease(&self, record: &SandboxLeaseRecord) -> Result<(), String> {
        let options = ReplaceOptions::builder().upsert(true).build();
        self.leases
            .replace_one(doc! { "id": &record.id }, record, options)
            .await
            .map(|_| ())
            .map_err(|err| format!("replace sandbox lease failed: {err}"))
    }

    pub async fn get_by_lease_id(
        &self,
        lease_id: &str,
    ) -> Result<Option<SandboxLeaseRecord>, String> {
        self.leases
            .find_one(doc! { "id": lease_id }, None)
            .await
            .map_err(|err| format!("get sandbox lease failed: {err}"))
    }

    pub async fn get_by_sandbox_id(
        &self,
        sandbox_id: &str,
    ) -> Result<Option<SandboxLeaseRecord>, String> {
        self.leases
            .find_one(doc! { "sandbox_id": sandbox_id }, None)
            .await
            .map_err(|err| format!("get sandbox failed: {err}"))
    }

    pub async fn list_leases(
        &self,
        query: ListSandboxQuery,
    ) -> Result<Vec<SandboxLeaseRecord>, String> {
        let mut filter = Document::new();
        insert_trimmed(&mut filter, "tenant_id", query.tenant_id);
        insert_trimmed(&mut filter, "user_id", query.user_id);
        insert_trimmed(&mut filter, "project_id", query.project_id);
        insert_trimmed(&mut filter, "run_id", query.run_id);
        if let Some(status) = query.status {
            let normalized = status.trim().to_ascii_lowercase();
            if !normalized.is_empty() {
                filter.insert("status", normalized);
            }
        }
        let options = FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .limit(query.limit.unwrap_or(100).clamp(1, 500))
            .build();
        self.leases
            .find(filter, options)
            .await
            .map_err(|err| format!("list sandbox leases failed: {err}"))?
            .try_collect()
            .await
            .map_err(|err| format!("read sandbox leases cursor failed: {err}"))
    }

    pub async fn list_expired_active(
        &self,
        now: &str,
        limit: i64,
    ) -> Result<Vec<SandboxLeaseRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "expires_at": 1 })
            .limit(limit.clamp(1, 200))
            .build();
        self.leases
            .find(
                doc! {
                    "expires_at": { "$lte": now },
                    "status": { "$in": active_status_strings() },
                },
                options,
            )
            .await
            .map_err(|err| format!("list expired sandboxes failed: {err}"))?
            .try_collect()
            .await
            .map_err(|err| format!("read expired sandboxes cursor failed: {err}"))
    }

    pub async fn append_event(&self, event: &SandboxEventRecord) -> Result<(), String> {
        self.events
            .insert_one(event, None)
            .await
            .map(|_| ())
            .map_err(|err| format!("insert sandbox event failed: {err}"))
    }

    pub async fn list_events(&self, sandbox_id: &str) -> Result<Vec<SandboxEventRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "created_at": 1 })
            .limit(500)
            .build();
        self.events
            .find(doc! { "sandbox_id": sandbox_id }, options)
            .await
            .map_err(|err| format!("list sandbox events failed: {err}"))?
            .try_collect()
            .await
            .map_err(|err| format!("read sandbox events cursor failed: {err}"))
    }
}

async fn create_index<T>(
    collection: &Collection<T>,
    keys: Document,
    name: Option<&str>,
    unique: bool,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder()
        .name(name.map(ToOwned::to_owned))
        .unique(unique)
        .build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model, None)
        .await
        .map(|_| ())
        .map_err(|err| format!("create mongodb index failed: {err}"))
}

fn insert_trimmed(filter: &mut Document, key: &str, value: Option<String>) {
    if let Some(value) = value.map(|value| value.trim().to_string()) {
        if !value.is_empty() {
            filter.insert(key, value);
        }
    }
}

fn active_status_strings() -> Vec<&'static str> {
    [
        SandboxStatus::Pending,
        SandboxStatus::Leasing,
        SandboxStatus::Starting,
        SandboxStatus::Ready,
        SandboxStatus::Running,
        SandboxStatus::Releasing,
        SandboxStatus::Destroying,
    ]
    .into_iter()
    .map(SandboxStatus::as_str)
    .collect()
}
