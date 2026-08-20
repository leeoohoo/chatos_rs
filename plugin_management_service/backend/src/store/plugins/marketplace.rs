// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn plugin_catalog_sync_event_from_document(
    document: Document,
) -> Result<PluginCatalogSyncOutboxEvent, String> {
    let marketplace_id = document
        .get_str("id")
        .map(str::to_string)
        .map_err(|_| "Plugin Catalog sync Outbox is missing marketplace id".to_string())?;
    let event_version = catalog_sync_event_version(&document).map_err(|_| {
        format!("Plugin Catalog sync Outbox for {marketplace_id} is missing event version")
    })?;
    let requested_at = document
        .get_str("catalog_sync_event_requested_at")
        .map(str::to_string)
        .map_err(|_| {
            format!("Plugin Catalog sync Outbox for {marketplace_id} is missing requested_at")
        })?;
    Ok(PluginCatalogSyncOutboxEvent {
        marketplace_id,
        event_version,
        requested_at,
        scheduled: document
            .get_bool("catalog_sync_event_scheduled")
            .unwrap_or(false),
    })
}

fn catalog_sync_event_version(
    document: &Document,
) -> Result<i64, mongodb::bson::document::ValueAccessError> {
    document.get_i64("catalog_sync_event_version").or_else(|_| {
        document
            .get_i32("catalog_sync_event_version")
            .map(i64::from)
    })
}

fn plugin_marketplace_update(
    record: &PluginMarketplaceRecord,
    request_sync: bool,
) -> Result<Document, String> {
    let mut set_fields = mongodb::bson::to_document(record).map_err(|err| err.to_string())?;
    if request_sync {
        set_fields.insert("catalog_sync_event_pending", true);
        set_fields.insert("catalog_sync_event_requested_at", now_rfc3339());
        set_fields.insert("catalog_sync_event_scheduled", false);
    } else {
        set_fields.insert("catalog_sync_event_pending", false);
    }
    let mut update = doc! { "$set": set_fields };
    if request_sync {
        update.insert("$inc", doc! { "catalog_sync_event_version": 1_i64 });
    }
    Ok(update)
}

impl AppStore {
    pub async fn list_plugin_marketplaces(&self) -> Result<Vec<PluginMarketplaceRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "enabled": -1, "trust_level": 1, "name": 1 })
            .build();
        self.plugin_marketplaces
            .find(None, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_plugin_marketplace(
        &self,
        id: &str,
    ) -> Result<Option<PluginMarketplaceRecord>, String> {
        self.plugin_marketplaces
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_plugin_marketplace_by_name(
        &self,
        name: &str,
    ) -> Result<Option<PluginMarketplaceRecord>, String> {
        self.plugin_marketplaces
            .find_one(doc! { "name": name }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_plugin_marketplace(
        &self,
        record: &PluginMarketplaceRecord,
    ) -> Result<(), String> {
        let fields = mongodb::bson::to_document(record).map_err(|err| err.to_string())?;
        self.plugin_marketplaces
            .update_one(
                doc! { "id": &record.id },
                doc! { "$set": fields },
                mongodb::options::UpdateOptions::builder()
                    .upsert(true)
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn replace_plugin_marketplace_with_catalog_sync(
        &self,
        record: &PluginMarketplaceRecord,
        request_sync: bool,
    ) -> Result<(), String> {
        self.plugin_marketplaces
            .update_one(
                doc! { "id": &record.id },
                plugin_marketplace_update(record, request_sync)?,
                mongodb::options::UpdateOptions::builder()
                    .upsert(true)
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn replace_plugin_marketplace_if_matches(
        &self,
        expected: &PluginMarketplaceRecord,
        record: &PluginMarketplaceRecord,
    ) -> Result<bool, String> {
        let filter = mongodb::bson::to_document(expected).map_err(|err| err.to_string())?;
        let fields = mongodb::bson::to_document(record).map_err(|err| err.to_string())?;
        let result = self
            .plugin_marketplaces
            .update_one(filter, doc! { "$set": fields }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.matched_count == 1)
    }

    pub async fn replace_plugin_marketplace_if_matches_with_catalog_sync(
        &self,
        expected: &PluginMarketplaceRecord,
        record: &PluginMarketplaceRecord,
        request_sync: bool,
    ) -> Result<bool, String> {
        let filter = mongodb::bson::to_document(expected).map_err(|err| err.to_string())?;
        let result = self
            .plugin_marketplaces
            .update_one(
                filter,
                plugin_marketplace_update(record, request_sync)?,
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.matched_count == 1)
    }

    pub async fn pending_plugin_catalog_sync_event(
        &self,
        marketplace_id: &str,
    ) -> Result<Option<PluginCatalogSyncOutboxEvent>, String> {
        let document = self
            .plugin_marketplace_documents
            .find_one(
                doc! {
                    "id": marketplace_id,
                    "catalog_sync_event_pending": true,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        document
            .map(plugin_catalog_sync_event_from_document)
            .transpose()
    }

    pub async fn list_pending_plugin_catalog_sync_events(
        &self,
        limit: i64,
    ) -> Result<Vec<PluginCatalogSyncOutboxEvent>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "catalog_sync_event_requested_at": 1, "id": 1 })
            .limit(Some(limit.clamp(1, 10_000)))
            .build();
        let documents: Vec<Document> = self
            .plugin_marketplace_documents
            .find(doc! { "catalog_sync_event_pending": true }, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        documents
            .into_iter()
            .map(plugin_catalog_sync_event_from_document)
            .collect()
    }

    pub async fn mark_plugin_catalog_sync_event_published(
        &self,
        event: &PluginCatalogSyncOutboxEvent,
    ) -> Result<bool, String> {
        let result = self
            .plugin_marketplace_documents
            .update_one(
                doc! {
                    "id": &event.marketplace_id,
                    "catalog_sync_event_version": event.event_version,
                    "catalog_sync_event_pending": true,
                },
                doc! {
                    "$set": {
                        "catalog_sync_event_pending": false,
                        "catalog_sync_event_published_version": event.event_version,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.modified_count == 1)
    }

    pub async fn plugin_catalog_sync_event_consumed(
        &self,
        marketplace_id: &str,
        event_version: i64,
    ) -> Result<Option<bool>, String> {
        let document = self
            .plugin_marketplace_documents
            .find_one(doc! { "id": marketplace_id }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(document.map(|document| {
            let current_version = catalog_sync_event_version(&document).unwrap_or(0);
            current_version > event_version
                || document
                    .get_i64("catalog_sync_event_consumed_version")
                    .unwrap_or(0)
                    >= event_version
        }))
    }

    pub async fn complete_plugin_catalog_sync_event(
        &self,
        event: &PluginCatalogSyncOutboxEvent,
        schedule_next: bool,
    ) -> Result<Option<PluginCatalogSyncOutboxEvent>, String> {
        let mut set_fields = doc! {
            "catalog_sync_event_consumed_version": event.event_version,
            "catalog_sync_event_pending": false,
        };
        let mut update = doc! { "$set": set_fields.clone() };
        if schedule_next {
            let requested_at = now_rfc3339();
            set_fields.insert("catalog_sync_event_pending", true);
            set_fields.insert("catalog_sync_event_requested_at", requested_at);
            set_fields.insert("catalog_sync_event_scheduled", true);
            update = doc! {
                "$set": set_fields,
                "$inc": { "catalog_sync_event_version": 1_i64 },
            };
        }
        let options = mongodb::options::FindOneAndUpdateOptions::builder()
            .return_document(mongodb::options::ReturnDocument::After)
            .build();
        let document = self
            .plugin_marketplace_documents
            .find_one_and_update(
                doc! {
                    "id": &event.marketplace_id,
                    "catalog_sync_event_version": event.event_version,
                    "$or": [
                        { "catalog_sync_event_consumed_version": { "$exists": false } },
                        { "catalog_sync_event_consumed_version": { "$lt": event.event_version } },
                    ],
                },
                update,
                options,
            )
            .await
            .map_err(|err| err.to_string())?;
        if !schedule_next {
            return Ok(None);
        }
        document
            .map(plugin_catalog_sync_event_from_document)
            .transpose()
    }

    pub async fn mark_plugin_catalog_sync_event_dead_lettered(
        &self,
        event: &PluginCatalogSyncOutboxEvent,
        error: &str,
    ) -> Result<bool, String> {
        let now = now_rfc3339();
        let result = self
            .plugin_marketplace_documents
            .update_one(
                doc! {
                    "id": &event.marketplace_id,
                    "catalog_sync_event_version": event.event_version,
                    "$or": [
                        { "catalog_sync_event_consumed_version": { "$exists": false } },
                        { "catalog_sync_event_consumed_version": { "$lt": event.event_version } },
                    ],
                },
                doc! {
                    "$set": {
                        "catalog_sync_event_consumed_version": event.event_version,
                        "catalog_sync_event_pending": false,
                        "catalog_sync_event_dead_letter_version": event.event_version,
                        "catalog_sync_event_dead_lettered_at": now,
                        "catalog_sync_event_last_error": error,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.modified_count == 1)
    }

    pub async fn replay_dead_lettered_plugin_catalog_sync(
        &self,
        marketplace_id: &str,
        dead_letter_version: i64,
    ) -> Result<Option<PluginCatalogSyncOutboxEvent>, String> {
        let options = mongodb::options::FindOneAndUpdateOptions::builder()
            .return_document(mongodb::options::ReturnDocument::After)
            .build();
        let document = self
            .plugin_marketplace_documents
            .find_one_and_update(
                doc! {
                    "id": marketplace_id,
                    "enabled": true,
                    "trust_level": PLUGIN_TRUST_TRUSTED,
                    "source_kind": {
                        "$in": [
                            PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY,
                            PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY,
                        ]
                    },
                    "catalog_url": { "$type": "string", "$ne": "" },
                    "catalog_sync_event_version": dead_letter_version,
                    "catalog_sync_event_dead_letter_version": dead_letter_version,
                    "catalog_sync_event_consumed_version": { "$gte": dead_letter_version },
                    "catalog_sync_event_pending": { "$ne": true },
                },
                doc! {
                    "$set": {
                        "catalog_sync_event_pending": true,
                        "catalog_sync_event_requested_at": now_rfc3339(),
                        "catalog_sync_event_scheduled": false,
                    },
                    "$inc": { "catalog_sync_event_version": 1_i64 },
                    "$unset": {
                        "catalog_sync_event_dead_letter_version": "",
                        "catalog_sync_event_dead_lettered_at": "",
                        "catalog_sync_event_last_error": "",
                    },
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?;
        document
            .map(plugin_catalog_sync_event_from_document)
            .transpose()
    }

    pub async fn recover_plugin_catalog_sync_events(&self, limit: i64) -> Result<u64, String> {
        let options = FindOptions::builder()
            .sort(doc! { "id": 1 })
            .limit(Some(limit.clamp(1, 10_000)))
            .build();
        let candidates: Vec<Document> = self
            .plugin_marketplace_documents
            .find(
                doc! {
                    "enabled": true,
                    "trust_level": PLUGIN_TRUST_TRUSTED,
                    "source_kind": {
                        "$in": [
                            PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY,
                            PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY,
                        ]
                    },
                    "catalog_url": { "$type": "string", "$ne": "" },
                    "$or": [
                        { "catalog_sync_event_version": { "$exists": false } },
                        {
                            "$expr": {
                                "$and": [
                                    {
                                        "$lte": [
                                            { "$ifNull": ["$catalog_sync_event_version", 0] },
                                            { "$ifNull": ["$catalog_sync_event_consumed_version", 0] },
                                        ]
                                    },
                                    {
                                        "$lt": [
                                            { "$ifNull": ["$catalog_sync_event_dead_letter_version", -1] },
                                            { "$ifNull": ["$catalog_sync_event_version", 0] },
                                        ]
                                    },
                                ]
                            }
                        },
                    ],
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        let mut recovered = 0_u64;
        for candidate in candidates {
            let Some(marketplace_id) = candidate.get_str("id").ok() else {
                continue;
            };
            let current_version = candidate.get_i64("catalog_sync_event_version").unwrap_or(0);
            let result = self
                .plugin_marketplace_documents
                .update_one(
                    doc! {
                        "id": marketplace_id,
                        "$expr": {
                            "$and": [
                                {
                                    "$lte": [
                                        { "$ifNull": ["$catalog_sync_event_version", 0] },
                                        { "$ifNull": ["$catalog_sync_event_consumed_version", 0] },
                                    ]
                                },
                                {
                                    "$lt": [
                                        { "$ifNull": ["$catalog_sync_event_dead_letter_version", -1] },
                                        { "$ifNull": ["$catalog_sync_event_version", 0] },
                                    ]
                                },
                            ]
                        },
                    },
                    doc! {
                        "$set": {
                            "catalog_sync_event_pending": true,
                            "catalog_sync_event_requested_at": now_rfc3339(),
                            "catalog_sync_event_scheduled": false,
                        },
                        "$inc": { "catalog_sync_event_version": 1_i64 },
                    },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            if result.modified_count == 1 {
                recovered += 1;
            } else if current_version == 0 {
                continue;
            }
        }
        Ok(recovered)
    }

    pub async fn acquire_plugin_catalog_sync_lease(
        &self,
        marketplace_id: &str,
        lock_owner: &str,
        lock_until: mongodb::bson::DateTime,
    ) -> Result<bool, String> {
        let now = mongodb::bson::DateTime::now();
        let result = self
            .plugin_marketplace_documents
            .update_one(
                doc! {
                    "id": marketplace_id,
                    "$or": [
                        { "catalog_sync_lock_until": { "$exists": false } },
                        { "catalog_sync_lock_until": { "$lte": now } },
                        { "catalog_sync_lock_owner": lock_owner },
                    ],
                },
                doc! {
                    "$set": {
                        "catalog_sync_lock_owner": lock_owner,
                        "catalog_sync_lock_until": lock_until,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.modified_count == 1)
    }

    pub async fn release_plugin_catalog_sync_lease(
        &self,
        marketplace_id: &str,
        lock_owner: &str,
    ) -> Result<(), String> {
        self.plugin_marketplace_documents
            .update_one(
                doc! {
                    "id": marketplace_id,
                    "catalog_sync_lock_owner": lock_owner,
                },
                doc! {
                    "$unset": {
                        "catalog_sync_lock_owner": "",
                        "catalog_sync_lock_until": "",
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn renew_plugin_catalog_sync_lease(
        &self,
        marketplace_id: &str,
        lock_owner: &str,
        lock_until: mongodb::bson::DateTime,
    ) -> Result<bool, String> {
        let result = self
            .plugin_marketplace_documents
            .update_one(
                doc! {
                    "id": marketplace_id,
                    "catalog_sync_lock_owner": lock_owner,
                },
                doc! { "$set": { "catalog_sync_lock_until": lock_until } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.modified_count == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::plugin_catalog_sync_event_from_document;
    use mongodb::bson::doc;

    #[test]
    fn parses_versioned_scheduled_catalog_sync_outbox() {
        let event = plugin_catalog_sync_event_from_document(doc! {
            "id": "marketplace-1",
            "catalog_sync_event_version": 3_i64,
            "catalog_sync_event_requested_at": "2026-08-05T00:00:00Z",
            "catalog_sync_event_scheduled": true,
        })
        .expect("parse Catalog sync Outbox");
        assert_eq!(event.marketplace_id, "marketplace-1");
        assert_eq!(event.event_version, 3);
        assert!(event.scheduled);
    }

    #[test]
    fn rejects_catalog_sync_outbox_without_version() {
        let error = plugin_catalog_sync_event_from_document(doc! {
            "id": "marketplace-1",
            "catalog_sync_event_requested_at": "2026-08-05T00:00:00Z",
        })
        .expect_err("missing event version must fail");
        assert!(error.contains("event version"));
    }
}
