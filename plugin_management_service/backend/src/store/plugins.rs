// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_plugin_management_sdk::PluginMcpCloudRuntimeBundle;

mod user_state;

impl AppStore {
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
        self.plugin_marketplaces
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
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
        let result = self
            .plugin_marketplaces
            .replace_one(filter, record, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.matched_count == 1)
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

    pub async fn get_plugin_cloud_component_bundle(
        &self,
        plugin_id: &str,
        release_id: &str,
        component_key: &str,
    ) -> Result<Option<PluginCloudComponentBundle>, String> {
        self.plugin_cloud_component_bundles
            .find_one(
                doc! {
                    "plugin_id": plugin_id,
                    "release_id": release_id,
                    "component_key": component_key,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_plugin_cloud_component_bundles(
        &self,
        plugin_id: &str,
        release_id: &str,
    ) -> Result<Vec<PluginCloudComponentBundle>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "component_key": 1 })
            .build();
        self.plugin_cloud_component_bundles
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

    pub async fn insert_plugin_cloud_component_bundles(
        &self,
        records: &[PluginCloudComponentBundle],
    ) -> Result<(), String> {
        for record in records {
            if let Some(existing) = self
                .get_plugin_cloud_component_bundle(
                    record.plugin_id.as_str(),
                    record.release_id.as_str(),
                    record.component_key.as_str(),
                )
                .await?
            {
                if existing != *record {
                    return Err(format!(
                        "immutable Plugin cloud Bundle conflict: {}/{}/{}",
                        record.plugin_id, record.release_id, record.component_key
                    ));
                }
                continue;
            }
            self.plugin_cloud_component_bundles
                .insert_one(record, None)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    pub async fn get_plugin_mcp_cloud_runtime_bundle(
        &self,
        plugin_id: &str,
        release_id: &str,
        component_key: &str,
    ) -> Result<Option<PluginMcpCloudRuntimeBundle>, String> {
        self.plugin_mcp_cloud_runtime_bundles
            .find_one(
                doc! {
                    "plugin_id": plugin_id,
                    "release_id": release_id,
                    "component.component_key": component_key,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_plugin_mcp_cloud_runtime_bundles(
        &self,
        plugin_id: &str,
        release_id: &str,
    ) -> Result<Vec<PluginMcpCloudRuntimeBundle>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "component.component_key": 1 })
            .build();
        self.plugin_mcp_cloud_runtime_bundles
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

    pub async fn insert_plugin_mcp_cloud_runtime_bundles(
        &self,
        records: &[PluginMcpCloudRuntimeBundle],
    ) -> Result<(), String> {
        for record in records {
            if let Some(existing) = self
                .get_plugin_mcp_cloud_runtime_bundle(
                    record.plugin_id.as_str(),
                    record.release_id.as_str(),
                    record.component.component_key.as_str(),
                )
                .await?
            {
                if existing != *record {
                    return Err(format!(
                        "immutable Plugin MCP cloud runtime Bundle conflict: {}/{}/{}",
                        record.plugin_id, record.release_id, record.component.component_key
                    ));
                }
                continue;
            }
            self.plugin_mcp_cloud_runtime_bundles
                .insert_one(record, None)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }
}
