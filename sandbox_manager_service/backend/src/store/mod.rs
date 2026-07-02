// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::{
    ClientOptions, FindOneAndUpdateOptions, FindOptions, IndexOptions, ReplaceOptions,
    ReturnDocument, UpdateOptions,
};
use mongodb::{Client, Collection, IndexModel};

use crate::models::{ListSandboxQuery, SandboxEventRecord, SandboxLeaseRecord, SandboxStatus};

#[derive(Clone)]
pub struct SandboxStore {
    leases: Collection<SandboxLeaseRecord>,
    events: Collection<SandboxEventRecord>,
    capacity_slots: Collection<Document>,
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
            capacity_slots: db.collection("sandbox_capacity_slots"),
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
        create_index_with_partial_filter(
            &self.leases,
            doc! { "tenant_id": 1, "project_id": 1, "run_id": 1, "idempotency_key": 1 },
            Some("idx_sandbox_leases_idempotency_key"),
            true,
            doc! { "idempotency_key": { "$exists": true } },
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
        create_index_with_options(
            &self.capacity_slots,
            doc! { "lease_id": 1 },
            Some("idx_sandbox_capacity_slots_lease"),
            true,
            true,
        )
        .await?;
        Ok(())
    }

    pub async fn try_acquire_active_slot(
        &self,
        max_active: usize,
        lease_id: &str,
        sandbox_id: &str,
        claim_until: &str,
    ) -> Result<bool, String> {
        let max_active = max_active.max(1);
        self.ensure_capacity_slots(max_active).await?;
        let now = chrono::Utc::now().to_rfc3339();
        let options = FindOneAndUpdateOptions::builder()
            .sort(doc! { "slot": 1 })
            .return_document(ReturnDocument::After)
            .build();
        let acquired = self
            .capacity_slots
            .find_one_and_update(
                doc! {
                    "_id": { "$in": capacity_slot_ids(max_active) },
                    "$or": [
                        { "lease_id": { "$exists": false } },
                        { "claim_until": { "$lte": now.as_str() } },
                    ],
                },
                doc! {
                    "$set": {
                        "lease_id": lease_id,
                        "sandbox_id": sandbox_id,
                        "claimed_at": now,
                        "claim_until": claim_until,
                    },
                    "$unset": {
                        "released_at": "",
                    },
                },
                options,
            )
            .await
            .map_err(|err| format!("acquire sandbox capacity slot failed: {err}"))?
            .is_some();
        Ok(acquired)
    }

    pub async fn extend_active_slot(
        &self,
        lease_id: &str,
        claim_until: &str,
    ) -> Result<(), String> {
        self.capacity_slots
            .update_one(
                doc! { "lease_id": lease_id },
                doc! { "$set": { "claim_until": claim_until } },
                None,
            )
            .await
            .map(|_| ())
            .map_err(|err| format!("extend sandbox capacity slot failed: {err}"))
    }

    pub async fn release_active_slot(&self, lease_id: &str) -> Result<bool, String> {
        let released_at = chrono::Utc::now().to_rfc3339();
        let result = self
            .capacity_slots
            .update_one(
                doc! { "lease_id": lease_id },
                doc! {
                    "$unset": {
                        "lease_id": "",
                        "sandbox_id": "",
                        "claim_until": "",
                    },
                    "$set": {
                        "released_at": released_at,
                    },
                },
                None,
            )
            .await
            .map_err(|err| format!("release sandbox capacity slot failed: {err}"))?;
        Ok(result.modified_count > 0)
    }

    pub async fn active_capacity_count(&self, max_active: usize) -> Result<usize, String> {
        let max_active = max_active.max(1);
        self.ensure_capacity_slots(max_active).await?;
        let now = chrono::Utc::now().to_rfc3339();
        self.capacity_slots
            .count_documents(
                doc! {
                    "_id": { "$in": capacity_slot_ids(max_active) },
                    "lease_id": { "$exists": true },
                    "claim_until": { "$gt": now },
                },
                None,
            )
            .await
            .map(|count| count as usize)
            .map_err(|err| format!("count sandbox capacity slots failed: {err}"))
    }

    pub async fn reconcile_active_capacity_slots(
        &self,
        max_active: usize,
    ) -> Result<usize, String> {
        let max_active = max_active.max(1);
        self.ensure_capacity_slots(max_active).await?;
        let now = chrono::Utc::now().to_rfc3339();
        let options = FindOptions::builder()
            .sort(doc! { "created_at": 1 })
            .limit(max_active as i64)
            .build();
        let active_leases: Vec<SandboxLeaseRecord> = self
            .leases
            .find(
                doc! {
                    "expires_at": { "$gt": now.as_str() },
                    "status": { "$in": active_status_strings() },
                },
                options,
            )
            .await
            .map_err(|err| {
                format!("list active sandbox leases for capacity reconcile failed: {err}")
            })?
            .try_collect()
            .await
            .map_err(|err| {
                format!("read active sandbox leases for capacity reconcile failed: {err}")
            })?;

        let mut reconciled = 0usize;
        for lease in active_leases {
            if self
                .capacity_slots
                .find_one(doc! { "lease_id": lease.id.as_str() }, None)
                .await
                .map_err(|err| format!("check sandbox capacity slot failed: {err}"))?
                .is_some()
            {
                continue;
            }
            match self
                .try_acquire_active_slot(
                    max_active,
                    lease.id.as_str(),
                    lease.sandbox_id.as_str(),
                    lease.expires_at.as_str(),
                )
                .await
            {
                Ok(true) => reconciled += 1,
                Ok(false) => {}
                Err(err) if is_duplicate_key_error(&err) => {}
                Err(err) => return Err(err),
            }
        }
        Ok(reconciled)
    }

    async fn ensure_capacity_slots(&self, max_active: usize) -> Result<(), String> {
        let options = UpdateOptions::builder().upsert(true).build();
        for slot in 0..max_active.max(1) {
            self.capacity_slots
                .update_one(
                    doc! { "_id": capacity_slot_id(slot) },
                    doc! {
                        "$setOnInsert": {
                            "slot": slot as i64,
                            "created_at": chrono::Utc::now().to_rfc3339(),
                        },
                    },
                    options.clone(),
                )
                .await
                .map_err(|err| format!("ensure sandbox capacity slot failed: {err}"))?;
        }
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

    pub async fn get_by_idempotency_key(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<SandboxLeaseRecord>, String> {
        self.leases
            .find_one(
                doc! {
                    "tenant_id": tenant_id,
                    "project_id": project_id,
                    "run_id": run_id,
                    "idempotency_key": idempotency_key,
                },
                None,
            )
            .await
            .map_err(|err| format!("get sandbox lease by idempotency key failed: {err}"))
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
    create_index_with_options(collection, keys, name, unique, false).await
}

async fn create_index_with_options<T>(
    collection: &Collection<T>,
    keys: Document,
    name: Option<&str>,
    unique: bool,
    sparse: bool,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder()
        .name(name.map(ToOwned::to_owned))
        .unique(unique)
        .sparse(sparse)
        .build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model, None)
        .await
        .map(|_| ())
        .map_err(|err| format!("create mongodb index failed: {err}"))
}

async fn create_index_with_partial_filter<T>(
    collection: &Collection<T>,
    keys: Document,
    name: Option<&str>,
    unique: bool,
    partial_filter_expression: Document,
) -> Result<(), String>
where
    T: Send + Sync,
{
    let options = IndexOptions::builder()
        .name(name.map(ToOwned::to_owned))
        .unique(unique)
        .partial_filter_expression(partial_filter_expression)
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

fn capacity_slot_id(slot: usize) -> String {
    format!("active:{slot}")
}

fn capacity_slot_ids(max_active: usize) -> Vec<String> {
    (0..max_active.max(1)).map(capacity_slot_id).collect()
}

pub(crate) fn is_duplicate_key_error(message: &str) -> bool {
    message.contains("E11000") || message.to_ascii_lowercase().contains("duplicate key")
}
