// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppStore {
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

    pub async fn list_plugin_cloud_credentials(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        release_id: &str,
        component_key: &str,
    ) -> Result<Vec<StoredPluginCloudCredential>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "secret_name": 1 })
            .build();
        self.plugin_cloud_credentials
            .find(
                doc! {
                    "owner_user_id": owner_user_id,
                    "plugin_id": plugin_id,
                    "release_id": release_id,
                    "component_key": component_key,
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_plugin_cloud_credential(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        release_id: &str,
        component_key: &str,
        secret_name: &str,
    ) -> Result<Option<StoredPluginCloudCredential>, String> {
        self.plugin_cloud_credentials
            .find_one(
                doc! {
                    "owner_user_id": owner_user_id,
                    "plugin_id": plugin_id,
                    "release_id": release_id,
                    "component_key": component_key,
                    "secret_name": secret_name,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_plugin_cloud_credential(
        &self,
        record: &StoredPluginCloudCredential,
    ) -> Result<(), String> {
        self.plugin_cloud_credentials
            .replace_one(doc! { "id": &record.metadata.id }, record, upsert_options())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn delete_plugin_cloud_credential(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        release_id: &str,
        component_key: &str,
        secret_name: &str,
    ) -> Result<bool, String> {
        self.plugin_cloud_credentials
            .delete_one(
                doc! {
                    "owner_user_id": owner_user_id,
                    "plugin_id": plugin_id,
                    "release_id": release_id,
                    "component_key": component_key,
                    "secret_name": secret_name,
                },
                None,
            )
            .await
            .map(|result| result.deleted_count == 1)
            .map_err(|err| err.to_string())
    }

    pub async fn list_plugin_cloud_oauth_connections(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        release_id: &str,
    ) -> Result<Vec<StoredPluginCloudOAuthConnection>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "component_key": 1, "provider": 1, "resource": 1 })
            .build();
        self.plugin_cloud_oauth_connections
            .find(
                doc! {
                    "owner_user_id": owner_user_id,
                    "plugin_id": plugin_id,
                    "release_id": release_id,
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn get_plugin_cloud_oauth_connection(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        release_id: &str,
        component_key: &str,
        provider: &str,
        resource: &str,
    ) -> Result<Option<StoredPluginCloudOAuthConnection>, String> {
        self.plugin_cloud_oauth_connections
            .find_one(
                doc! {
                    "owner_user_id": owner_user_id,
                    "plugin_id": plugin_id,
                    "release_id": release_id,
                    "component_key": component_key,
                    "provider": provider,
                    "resource": resource,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn replace_plugin_cloud_oauth_connection(
        &self,
        record: &StoredPluginCloudOAuthConnection,
    ) -> Result<(), String> {
        self.plugin_cloud_oauth_connections
            .replace_one(
                doc! { "id": &record.connection.id },
                record,
                upsert_options(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn replace_claimed_plugin_cloud_oauth_connection(
        &self,
        record: &StoredPluginCloudOAuthConnection,
        refresh_lease_id: &str,
    ) -> Result<bool, String> {
        self.plugin_cloud_oauth_connections
            .replace_one(
                doc! {
                    "id": &record.connection.id,
                    "refresh_lease_id": refresh_lease_id,
                },
                record,
                None,
            )
            .await
            .map(|result| result.modified_count == 1)
            .map_err(|err| err.to_string())
    }

    pub async fn get_plugin_cloud_oauth_connection_by_id(
        &self,
        connection_id: &str,
    ) -> Result<Option<StoredPluginCloudOAuthConnection>, String> {
        self.plugin_cloud_oauth_connections
            .find_one(doc! { "id": connection_id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn claim_plugin_cloud_oauth_refresh(
        &self,
        connection_id: &str,
        expected_revision: &str,
        lease_id: &str,
        now: mongodb::bson::DateTime,
        expires_at: mongodb::bson::DateTime,
    ) -> Result<bool, String> {
        self.plugin_cloud_oauth_connections
            .update_one(
                doc! {
                    "id": connection_id,
                    "revision": expected_revision,
                    "$or": [
                        { "refresh_lease_id": { "$exists": false } },
                        { "refresh_lease_id": null },
                        { "refresh_lease_expires_at": { "$lte": now } },
                    ],
                },
                doc! {
                    "$set": {
                        "refresh_lease_id": lease_id,
                        "refresh_lease_expires_at": expires_at,
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count == 1)
            .map_err(|err| err.to_string())
    }

    pub async fn release_plugin_cloud_oauth_refresh(
        &self,
        connection_id: &str,
        lease_id: &str,
    ) -> Result<(), String> {
        self.plugin_cloud_oauth_connections
            .update_one(
                doc! { "id": connection_id, "refresh_lease_id": lease_id },
                doc! {
                    "$unset": {
                        "refresh_lease_id": "",
                        "refresh_lease_expires_at": "",
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn insert_plugin_cloud_oauth_authorization(
        &self,
        record: &StoredPluginCloudOAuthAuthorizationSession,
    ) -> Result<(), String> {
        self.plugin_cloud_oauth_authorizations
            .insert_one(record, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn consume_plugin_cloud_oauth_authorization(
        &self,
        state_sha256: &str,
    ) -> Result<Option<StoredPluginCloudOAuthAuthorizationSession>, String> {
        self.plugin_cloud_oauth_authorizations
            .find_one_and_delete(doc! { "state_sha256": state_sha256 }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn delete_plugin_cloud_oauth_connection(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        connection_id: &str,
    ) -> Result<bool, String> {
        self.plugin_cloud_oauth_connections
            .delete_one(
                doc! {
                    "owner_user_id": owner_user_id,
                    "plugin_id": plugin_id,
                    "id": connection_id,
                },
                None,
            )
            .await
            .map(|result| result.deleted_count == 1)
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
