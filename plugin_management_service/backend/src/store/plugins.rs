// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
        self.plugin_releases
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_plugin_release(
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

    pub async fn list_plugin_installations(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<Vec<PluginInstallationRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "active": -1, "last_checked_at": -1 })
            .build();
        self.plugin_installations
            .find(
                doc! { "owner_user_id": owner_user_id, "device_id": device_id },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_plugin_installation(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
    ) -> Result<Option<PluginInstallationRecord>, String> {
        self.plugin_installations
            .find_one(
                doc! {
                    "owner_user_id": owner_user_id,
                    "device_id": device_id,
                    "plugin_id": plugin_id,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_plugin_installation(
        &self,
        record: &PluginInstallationRecord,
    ) -> Result<(), String> {
        self.plugin_installations
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_user_plugin_preference(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
    ) -> Result<Option<UserPluginPreferenceRecord>, String> {
        self.plugin_preferences
            .find_one(
                doc! { "owner_user_id": owner_user_id, "plugin_id": plugin_id },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_user_plugin_preference(
        &self,
        record: &UserPluginPreferenceRecord,
    ) -> Result<(), String> {
        self.plugin_preferences
            .replace_one(
                doc! { "owner_user_id": &record.owner_user_id, "plugin_id": &record.plugin_id },
                record,
                upsert_options(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_plugin_oauth_connections(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
    ) -> Result<Vec<PluginOAuthConnectionRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "provider": 1, "component_key": 1 })
            .build();
        self.plugin_oauth_connections
            .find(
                doc! {
                    "owner_user_id": owner_user_id,
                    "device_id": device_id,
                    "plugin_id": plugin_id,
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_plugin_oauth_connection(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
        component_key: &str,
        provider: &str,
    ) -> Result<Option<PluginOAuthConnectionRecord>, String> {
        self.plugin_oauth_connections
            .find_one(
                doc! {
                    "owner_user_id": owner_user_id,
                    "device_id": device_id,
                    "plugin_id": plugin_id,
                    "component_key": component_key,
                    "provider": provider,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_plugin_oauth_connection(
        &self,
        record: &PluginOAuthConnectionRecord,
    ) -> Result<(), String> {
        self.plugin_oauth_connections
            .replace_one(doc! { "id": &record.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn insert_plugin_audit(&self, record: &PluginAuditLogRecord) -> Result<(), String> {
        self.plugin_audit_logs
            .insert_one(record, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_plugin_audit(
        &self,
        query: &PluginAuditQuery,
    ) -> Result<ListResponse<PluginAuditLogRecord>, String> {
        let mut filter = doc! {};
        for (field, value) in [
            ("plugin_id", query.plugin_id.as_deref()),
            ("owner_user_id", query.owner_user_id.as_deref()),
            ("device_id", query.device_id.as_deref()),
            ("event", query.event.as_deref()),
        ] {
            if let Some(value) = normalized(value) {
                filter.insert(field, value);
            }
        }
        let total = self
            .plugin_audit_logs
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())?;
        let options = FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .limit(Some(query.limit.unwrap_or(100).clamp(1, 500)))
            .skip(query.offset)
            .build();
        let items = self
            .plugin_audit_logs
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        Ok(ListResponse { items, total })
    }
}
