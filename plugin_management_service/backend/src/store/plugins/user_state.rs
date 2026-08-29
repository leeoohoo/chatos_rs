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

    pub async fn get_preferred_plugin_installation(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
    ) -> Result<Option<PluginInstallationRecord>, String> {
        let options = mongodb::options::FindOneOptions::builder()
            .sort(doc! { "last_checked_at": -1, "installed_at": -1, "id": 1 })
            .build();
        self.plugin_installations
            .find_one(
                preferred_plugin_installation_filter(owner_user_id, plugin_id),
                options,
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

    pub async fn list_enabled_user_plugin_preferences(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<UserPluginPreferenceRecord>, String> {
        self.plugin_preferences
            .find(
                doc! { "owner_user_id": owner_user_id, "enabled": true },
                None,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
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

fn preferred_plugin_installation_filter(owner_user_id: &str, plugin_id: &str) -> Document {
    doc! {
        "owner_user_id": owner_user_id,
        "plugin_id": plugin_id,
        "active": true,
        "install_status": "installed",
        "availability_status": { "$in": ["ready", "partially_available"] },
        "dependency_status": "satisfied",
        "permission_status": "satisfied",
        "auth_status": "satisfied",
    }
}

#[cfg(test)]
mod tests {
    use super::preferred_plugin_installation_filter;

    #[test]
    fn preferred_installation_query_is_scoped_to_the_exact_owner_and_plugin() {
        let filter = preferred_plugin_installation_filter("owner-1", "plugin-browser");

        assert_eq!(filter.get_str("owner_user_id"), Ok("owner-1"));
        assert_eq!(filter.get_str("plugin_id"), Ok("plugin-browser"));
        assert_eq!(filter.get_bool("active"), Ok(true));
        assert_eq!(filter.get_str("install_status"), Ok("installed"));
    }
}
