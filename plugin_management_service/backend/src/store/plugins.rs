// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod marketplace;
mod user_state;

const RETIRED_BUNDLED_MARKETPLACE_ID: &str = "chatos-bundled";

impl AppStore {
    pub async fn remove_retired_bundled_plugin_marketplaces(&self) -> Result<u64, String> {
        let marketplace_documents = self
            .plugin_marketplace_documents
            .find(
                doc! {
                    "$or": [
                        { "id": RETIRED_BUNDLED_MARKETPLACE_ID },
                        { "trust_level": "bundled" },
                    ],
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect::<Vec<Document>>()
            .await
            .map_err(|err| err.to_string())?;
        let mut marketplace_ids = marketplace_documents
            .iter()
            .filter_map(|document| document.get_str("id").ok())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if !marketplace_ids
            .iter()
            .any(|id| id == RETIRED_BUNDLED_MARKETPLACE_ID)
        {
            marketplace_ids.push(RETIRED_BUNDLED_MARKETPLACE_ID.to_string());
        }

        let catalog_documents = self
            .database
            .collection::<Document>("plugin_catalog_entries")
            .find(doc! { "marketplace_id": { "$in": &marketplace_ids } }, None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect::<Vec<Document>>()
            .await
            .map_err(|err| err.to_string())?;
        let plugin_ids = catalog_documents
            .iter()
            .filter_map(|document| document.get_str("id").ok())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let release_documents = if plugin_ids.is_empty() {
            Vec::new()
        } else {
            self.database
                .collection::<Document>("plugin_releases")
                .find(doc! { "plugin_id": { "$in": &plugin_ids } }, None)
                .await
                .map_err(|err| err.to_string())?
                .try_collect::<Vec<Document>>()
                .await
                .map_err(|err| err.to_string())?
        };
        let release_ids = release_documents
            .iter()
            .filter_map(|document| document.get_str("id").ok())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        if !plugin_ids.is_empty() {
            self.bindings
                .delete_many(
                    doc! {
                        "resource_kind": {
                            "$in": [RESOURCE_KIND_PLUGIN, RESOURCE_KIND_PLUGIN_COMPONENT]
                        },
                        "resource_id": { "$in": &plugin_ids },
                    },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            self.plugin_installations
                .delete_many(doc! { "plugin_id": { "$in": &plugin_ids } }, None)
                .await
                .map_err(|err| err.to_string())?;
            self.plugin_preferences
                .delete_many(doc! { "plugin_id": { "$in": &plugin_ids } }, None)
                .await
                .map_err(|err| err.to_string())?;
            self.plugin_component_snapshots
                .delete_many(doc! { "plugin_id": { "$in": &plugin_ids } }, None)
                .await
                .map_err(|err| err.to_string())?;
            self.plugin_oauth_connections
                .delete_many(doc! { "plugin_id": { "$in": &plugin_ids } }, None)
                .await
                .map_err(|err| err.to_string())?;
            self.plugin_audit_logs
                .delete_many(doc! { "plugin_id": { "$in": &plugin_ids } }, None)
                .await
                .map_err(|err| err.to_string())?;
            self.plugin_releases
                .delete_many(doc! { "plugin_id": { "$in": &plugin_ids } }, None)
                .await
                .map_err(|err| err.to_string())?;
        }
        if !release_ids.is_empty() {
            self.plugin_release_publication_states
                .delete_many(doc! { "release_id": { "$in": &release_ids } }, None)
                .await
                .map_err(|err| err.to_string())?;
        }

        self.plugin_catalog_entries
            .delete_many(doc! { "marketplace_id": { "$in": &marketplace_ids } }, None)
            .await
            .map_err(|err| err.to_string())?;
        self.plugin_publishers
            .delete_many(doc! { "marketplace_id": { "$in": &marketplace_ids } }, None)
            .await
            .map_err(|err| err.to_string())?;
        self.plugin_catalog_syncs
            .delete_many(doc! { "marketplace_id": { "$in": &marketplace_ids } }, None)
            .await
            .map_err(|err| err.to_string())?;
        self.plugin_audit_logs
            .delete_many(
                doc! {
                    "plugin_id": {
                        "$in": marketplace_ids
                            .iter()
                            .map(|id| format!("marketplace:{id}"))
                            .collect::<Vec<_>>()
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.plugin_marketplaces
            .delete_many(doc! { "id": { "$in": &marketplace_ids } }, None)
            .await
            .map(|result| result.deleted_count)
            .map_err(|err| err.to_string())
    }

    pub async fn delete_plugin_bindings_for_agent(&self, agent_key: &str) -> Result<(), String> {
        self.bindings
            .delete_many(
                doc! {
                    "agent_key": agent_key,
                    "resource_kind": {
                        "$in": [RESOURCE_KIND_PLUGIN, RESOURCE_KIND_PLUGIN_COMPONENT]
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_plugin_publishers(
        &self,
        query: &PluginPublisherQuery,
        owner_user_id: Option<&str>,
    ) -> Result<ListResponse<PluginPublisherRecord>, String> {
        let mut filter = doc! {};
        if let Some(owner_user_id) = owner_user_id {
            filter.insert("owner_user_id", owner_user_id);
        }
        if let Some(marketplace_id) = normalized(query.marketplace_id.as_deref()) {
            filter.insert("marketplace_id", marketplace_id);
        }
        if let Some(status) = normalized(query.status.as_deref()) {
            filter.insert("status", status);
        }
        let total = self
            .plugin_publishers
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())?;
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1, "created_at": -1 })
            .limit(Some(query.limit.unwrap_or(100).clamp(1, 500)))
            .skip(query.offset)
            .build();
        let items = self
            .plugin_publishers
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        Ok(ListResponse { items, total })
    }

    pub async fn get_plugin_publisher(
        &self,
        id: &str,
    ) -> Result<Option<PluginPublisherRecord>, String> {
        self.plugin_publishers
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_plugin_publisher(
        &self,
        marketplace_id: &str,
        publisher_id: &str,
    ) -> Result<Option<PluginPublisherRecord>, String> {
        self.plugin_publishers
            .find_one(
                doc! { "marketplace_id": marketplace_id, "publisher_id": publisher_id },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_plugin_publisher(
        &self,
        record: &PluginPublisherRecord,
    ) -> Result<(), String> {
        self.plugin_publishers
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn replace_plugin_publisher_if_matches(
        &self,
        expected: &PluginPublisherRecord,
        record: &PluginPublisherRecord,
    ) -> Result<bool, String> {
        let filter = mongodb::bson::to_document(expected).map_err(|err| err.to_string())?;
        let result = self
            .plugin_publishers
            .replace_one(filter, record, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.matched_count == 1)
    }

    pub async fn get_plugin_catalog_sync(
        &self,
        marketplace_id: &str,
    ) -> Result<Option<PluginCatalogSyncRecord>, String> {
        self.plugin_catalog_syncs
            .find_one(doc! { "marketplace_id": marketplace_id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn commit_plugin_catalog_sync(
        &self,
        record: &PluginCatalogSyncRecord,
        expected_revision: Option<&str>,
    ) -> Result<bool, String> {
        if let Some(expected_revision) = expected_revision {
            let result = self
                .plugin_catalog_syncs
                .replace_one(
                    doc! {
                        "marketplace_id": &record.marketplace_id,
                        "revision": expected_revision,
                    },
                    record,
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            return Ok(result.matched_count == 1);
        }
        match self.plugin_catalog_syncs.insert_one(record, None).await {
            Ok(_) => Ok(true),
            Err(error) if error.to_string().contains("E11000") => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    }

    pub async fn list_plugin_catalog(
        &self,
        query: &PluginCatalogQuery,
        visible_owner_user_id: Option<&str>,
    ) -> Result<ListResponse<PluginCatalogRecord>, String> {
        let mut filter = doc! {};
        let mut predicates = Vec::new();
        if let Some(owner_user_id) = visible_owner_user_id {
            filter.insert("enabled", true);
            predicates.push(doc! {
                "$or": [
                    { "visibility": PLUGIN_VISIBILITY_PUBLIC },
                    {
                        "visibility": PLUGIN_VISIBILITY_PRIVATE,
                        "owner_user_id": owner_user_id,
                    },
                ]
            });
        } else {
            if let Some(visibility) = normalized(query.visibility.as_deref()) {
                filter.insert("visibility", visibility);
            }
            if let Some(enabled) = query.enabled {
                filter.insert("enabled", enabled);
            }
        }
        if let Some(marketplace_id) = normalized(query.marketplace_id.as_deref()) {
            filter.insert("marketplace_id", marketplace_id);
        }
        if let Some(category) = normalized(query.category.as_deref()) {
            filter.insert("interface.category", category);
        }
        if let Some(featured) = query.featured {
            filter.insert("featured", featured);
        }
        if let Some(q) = normalized(query.q.as_deref()) {
            let regex = Regex {
                pattern: q,
                options: "i".to_string(),
            };
            predicates.push(doc! {
                "$or": [
                    doc! { "id": { "$regex": regex.clone() } },
                    doc! { "name": { "$regex": regex.clone() } },
                    doc! { "display_name": { "$regex": regex.clone() } },
                    doc! { "description": { "$regex": regex.clone() } },
                    doc! { "keywords": { "$regex": regex } },
                ]
            });
        }
        if !predicates.is_empty() {
            filter.insert("$and", predicates);
        }
        let total = self
            .plugin_catalog_entries
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())?;
        let options = FindOptions::builder()
            .sort(doc! { "featured": -1, "interface.category": 1, "display_name": 1 })
            .limit(Some(query.limit.unwrap_or(100).clamp(1, 500)))
            .skip(query.offset)
            .build();
        let items = self
            .plugin_catalog_entries
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        Ok(ListResponse { items, total })
    }

    pub async fn get_plugin_catalog_entry(
        &self,
        id: &str,
    ) -> Result<Option<PluginCatalogRecord>, String> {
        self.plugin_catalog_entries
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_plugin_catalog_entry(
        &self,
        marketplace_id: &str,
        name: &str,
    ) -> Result<Option<PluginCatalogRecord>, String> {
        self.plugin_catalog_entries
            .find_one(
                doc! { "marketplace_id": marketplace_id, "name": name },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_plugin_catalog_entry(
        &self,
        record: &PluginCatalogRecord,
    ) -> Result<(), String> {
        self.plugin_catalog_entries
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_plugin_releases(
        &self,
        plugin_id: &str,
        include_revoked: bool,
    ) -> Result<Vec<PluginReleaseRecord>, String> {
        let mut filter = doc! { "plugin_id": plugin_id };
        if !include_revoked {
            filter.insert("revoked_at", doc! { "$eq": null });
        }
        let options = FindOptions::builder()
            .sort(doc! { "published_at": -1, "version": -1 })
            .build();
        let releases: Vec<PluginReleaseRecord> = self
            .plugin_releases
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        let mut ready = Vec::with_capacity(releases.len());
        for release in releases {
            if self.plugin_release_is_ready(release.id.as_str()).await? {
                ready.push(release);
            }
        }
        Ok(ready)
    }

    pub async fn get_plugin_release(
        &self,
        id: &str,
    ) -> Result<Option<PluginReleaseRecord>, String> {
        let release = self
            .plugin_releases
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())?;
        if release.is_some() && !self.plugin_release_is_ready(id).await? {
            return Ok(None);
        }
        Ok(release)
    }

    pub async fn list_plugin_releases_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<PluginReleaseRecord>, String> {
        let ids: Vec<String> = ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let releases: Vec<PluginReleaseRecord> = self
            .plugin_releases
            .find(doc! { "id": { "$in": ids } }, None)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        let mut ready = Vec::with_capacity(releases.len());
        for release in releases {
            if self.plugin_release_is_ready(release.id.as_str()).await? {
                ready.push(release);
            }
        }
        Ok(ready)
    }

    pub async fn get_plugin_release_any_state(
        &self,
        id: &str,
    ) -> Result<Option<PluginReleaseRecord>, String> {
        self.plugin_releases
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_plugin_release_by_version(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> Result<Option<PluginReleaseRecord>, String> {
        self.plugin_releases
            .find_one(doc! { "plugin_id": plugin_id, "version": version }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn insert_plugin_release(&self, record: &PluginReleaseRecord) -> Result<(), String> {
        self.plugin_releases
            .insert_one(record, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn set_plugin_release_publication_ready(
        &self,
        release_id: &str,
        ready: bool,
    ) -> Result<(), String> {
        let state = PluginReleasePublicationState {
            release_id: release_id.to_string(),
            ready,
            updated_at: now_rfc3339(),
        };
        self.plugin_release_publication_states
            .replace_one(doc! { "release_id": release_id }, state, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn plugin_release_is_ready(&self, release_id: &str) -> Result<bool, String> {
        self.plugin_release_publication_states
            .find_one(doc! { "release_id": release_id }, None)
            .await
            .map(|state| state.is_none_or(|state| state.ready))
            .map_err(|err| err.to_string())
    }

    pub async fn replace_plugin_release(&self, record: &PluginReleaseRecord) -> Result<(), String> {
        self.plugin_releases
            .replace_one(doc! { "id": &record.id }, record, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn replace_plugin_component_snapshots(
        &self,
        plugin_id: &str,
        release_id: &str,
        records: &[PluginComponentSnapshot],
    ) -> Result<(), String> {
        self.plugin_component_snapshots
            .delete_many(
                doc! { "plugin_id": plugin_id, "release_id": release_id },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if !records.is_empty() {
            self.plugin_component_snapshots
                .insert_many(records, None)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    pub async fn list_plugin_component_snapshots(
        &self,
        plugin_id: &str,
        release_id: &str,
    ) -> Result<Vec<PluginComponentSnapshot>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "component.component_key": 1 })
            .build();
        self.plugin_component_snapshots
            .find(
                doc! { "plugin_id": plugin_id, "release_id": release_id },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }
}
