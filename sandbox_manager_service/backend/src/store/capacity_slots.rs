// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument, UpdateOptions};

use crate::models::SandboxLeaseRecord;

use super::{active_status_strings, is_duplicate_key_error, SandboxStore};

impl SandboxStore {
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
}

fn capacity_slot_id(slot: usize) -> String {
    format!("active:{slot}")
}

fn capacity_slot_ids(max_active: usize) -> Vec<String> {
    (0..max_active.max(1)).map(capacity_slot_id).collect()
}
