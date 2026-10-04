// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
#[path = "maintenance_services.rs"]
mod maintenance_services;

impl AppState {
    async fn migrate_release_default_values<F>(
        &self,
        defaults: &BTreeMap<String, Value>,
        ensure_values: F,
    ) -> Result<BTreeMap<(String, i64), BTreeMap<String, Value>>, String>
    where
        F: Fn(
            &mut BTreeMap<String, Value>,
            &BTreeMap<String, Value>,
        ) -> Result<Vec<String>, String>,
    {
        let mut values_by_release = BTreeMap::new();
        for mut release in self.store.list_all_releases().await? {
            let changed_keys = ensure_values(&mut release.values, defaults)?;
            let effective_values = defaults
                .iter()
                .map(|(key, fallback)| {
                    (
                        key.clone(),
                        release
                            .values
                            .get(key)
                            .cloned()
                            .unwrap_or_else(|| fallback.clone()),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            values_by_release.insert(
                (release.environment.clone(), release.revision),
                effective_values,
            );
            if !changed_keys.is_empty() {
                for key in changed_keys {
                    ensure_changed_key(&mut release.changed_keys, key.as_str());
                }
                self.store.save_release(&release).await?;
            }
        }
        Ok(values_by_release)
    }

    pub(super) async fn migrate_postgres_pool_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = definitions
            .iter()
            .filter(|definition| {
                definition.key.starts_with("platform.postgres.")
                    || definition.key.contains(".postgres.")
            })
            .map(|definition| (definition.key.clone(), definition.default_value.clone()))
            .collect::<BTreeMap<_, _>>();
        if defaults.is_empty() {
            return Err("PostgreSQL pool configuration definitions are missing".to_string());
        }

        let mut values_by_release = BTreeMap::new();
        for mut release in self.store.list_all_releases().await? {
            let mut changed = Vec::new();
            for (key, fallback) in &defaults {
                if !release.values.contains_key(key) {
                    release.values.insert(key.clone(), fallback.clone());
                    ensure_changed_key(&mut release.changed_keys, key.as_str());
                    changed.push(key.clone());
                }
            }
            values_by_release.insert(
                (release.environment.clone(), release.revision),
                release.values.clone(),
            );
            if !changed.is_empty() {
                self.store.save_release(&release).await?;
            }
        }

        for snapshot in self.store.list_all_snapshots().await? {
            let all_values = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .ok_or_else(|| {
                    format!(
                        "release values are unavailable for PostgreSQL snapshot {}/{} revision {}",
                        snapshot.environment, snapshot.service_name, snapshot.revision
                    )
                })?;
            let mut rebuilt = build_snapshot(
                snapshot.environment.as_str(),
                snapshot.service_name.as_str(),
                snapshot.revision,
                &definitions,
                all_values,
            )?;
            rebuilt.generated_at = snapshot.generated_at.clone();
            if rebuilt.values != snapshot.values
                || rebuilt.env != snapshot.env
                || rebuilt.checksum != snapshot.checksum
            {
                self.store.save_snapshot(&rebuilt).await?;
            }
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "add managed PostgreSQL pool configuration",
        )
        .await?;
        tracing::info!(
            definition_count = defaults.len(),
            "PostgreSQL pool configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn audit(
        &self,
        environment: Option<&str>,
        action: &str,
        user: &CurrentUser,
        release_id: Option<&str>,
        changed_keys: Vec<String>,
        detail: Option<Value>,
    ) -> Result<(), String> {
        self.store
            .insert_audit(&AuditEventRecord {
                id: Uuid::new_v4().to_string(),
                environment: environment.map(ToOwned::to_owned),
                action: action.to_string(),
                actor_user_id: user.user_id.clone(),
                actor_display_name: user.display_name.clone(),
                release_id: release_id.map(ToOwned::to_owned),
                changed_keys,
                detail,
                created_at: Utc::now().to_rfc3339(),
            })
            .await
    }

    pub async fn heartbeat(
        &self,
        instance: ServiceInstanceRecord,
    ) -> Result<ServiceInstanceRecord, String> {
        self.store.upsert_instance(&instance).await?;
        Ok(instance)
    }

    pub(super) async fn purge_user_preferences_from_config_center(&self) -> Result<(), String> {
        for mut release in self.store.list_all_releases().await? {
            let mut changed = false;
            for key in USER_PREFERENCE_CONFIG_KEYS {
                changed |= release.values.remove(*key).is_some();
            }
            let previous_len = release.changed_keys.len();
            release
                .changed_keys
                .retain(|key| !USER_PREFERENCE_CONFIG_KEYS.contains(&key.as_str()));
            changed |= release.changed_keys.len() != previous_len;
            if changed {
                self.store.save_release(&release).await?;
            }
        }

        for mut snapshot in self.store.list_all_snapshots().await? {
            let mut changed = false;
            for key in USER_PREFERENCE_CONFIG_KEYS {
                changed |= snapshot.values.remove(*key).is_some();
            }
            changed |= snapshot.env.remove("UI_LOCALE").is_some();
            changed |= snapshot.env.remove("INTERNAL_CONTEXT_LOCALE").is_some();
            if changed {
                snapshot.checksum = checksum(&json!({
                    "values": snapshot.values,
                    "env": snapshot.env,
                }))?;
                self.store.save_snapshot(&snapshot).await?;
            }
        }

        for mut draft in self.store.list_drafts().await? {
            let mut had_user_preferences = false;
            for key in USER_PREFERENCE_CONFIG_KEYS {
                had_user_preferences |= draft.changes.remove(*key).is_some();
            }
            if had_user_preferences {
                draft.validation_status = "pending".to_string();
                draft.validation_errors.clear();
                draft.updated_at = Utc::now().to_rfc3339();
                self.store.save_draft(&draft).await?;
            }
        }

        for mut event in self.store.list_all_audit().await? {
            let previous_len = event.changed_keys.len();
            event
                .changed_keys
                .retain(|key| !USER_PREFERENCE_CONFIG_KEYS.contains(&key.as_str()));
            if event.changed_keys.len() != previous_len {
                self.store.save_audit(&event).await?;
            }
        }

        let definitions = self.store.list_definitions().await?;
        self.republish_active_releases_to_consul(&definitions, "remove user preferences")
            .await?;
        Ok(())
    }

    pub(super) async fn purge_retired_config_keys(&self) -> Result<(), String> {
        for mut release in self.store.list_all_releases().await? {
            let previous_values_len = release.values.len();
            release
                .values
                .retain(|key, _| !RETIRED_CONFIG_KEYS.contains(&key.as_str()));
            let previous_changed_keys_len = release.changed_keys.len();
            release
                .changed_keys
                .retain(|key| !RETIRED_CONFIG_KEYS.contains(&key.as_str()));
            if release.values.len() != previous_values_len
                || release.changed_keys.len() != previous_changed_keys_len
            {
                self.store.save_release(&release).await?;
            }
        }

        let definitions = self.store.list_definitions().await?;
        for mut snapshot in self.store.list_all_snapshots().await? {
            let previous_values_len = snapshot.values.len();
            snapshot
                .values
                .retain(|key, _| !RETIRED_CONFIG_KEYS.contains(&key.as_str()));
            let previous_env = snapshot.env.clone();
            snapshot.env = compatibility_env(&definitions, &snapshot.values, |definition| {
                definition.scope == "shared"
                    || definition.service_name.as_deref() == Some(snapshot.service_name.as_str())
            });
            if snapshot.values.len() != previous_values_len || snapshot.env != previous_env {
                snapshot.checksum = checksum(&json!({
                    "values": snapshot.values,
                    "env": snapshot.env,
                }))?;
                self.store.save_snapshot(&snapshot).await?;
            }
        }

        for mut draft in self.store.list_drafts().await? {
            let previous_len = draft.changes.len();
            draft
                .changes
                .retain(|key, _| !RETIRED_CONFIG_KEYS.contains(&key.as_str()));
            if draft.changes.len() != previous_len {
                draft.validation_status = "pending".to_string();
                draft.validation_errors.clear();
                draft.updated_at = Utc::now().to_rfc3339();
                self.store.save_draft(&draft).await?;
            }
        }

        for mut event in self.store.list_all_audit().await? {
            let previous_len = event.changed_keys.len();
            event
                .changed_keys
                .retain(|key| !RETIRED_CONFIG_KEYS.contains(&key.as_str()));
            if event.changed_keys.len() != previous_len {
                self.store.save_audit(&event).await?;
            }
        }

        self.republish_active_releases_to_consul(&definitions, "remove retired configuration")
            .await?;
        tracing::info!(
            retired_key_count = RETIRED_CONFIG_KEYS.len(),
            "retired configuration has been removed from configuration center"
        );
        Ok(())
    }

    pub(super) async fn migrate_local_connector_runtime_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = local_connector_service_runtime_default_values(&definitions);
        if defaults.is_empty() {
            return Err(
                "Local Connector runtime configuration definitions are incomplete".to_string(),
            );
        }
        let values_by_release = self
            .migrate_release_default_values(&defaults, |values, defaults| {
                Ok(ensure_local_connector_runtime_values(values, defaults))
            })
            .await?;

        for mut snapshot in self.store.list_all_snapshots().await? {
            if snapshot.service_name != "local-connector-service" {
                continue;
            }
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_local_connector_runtime_values(&mut snapshot.values, &snapshot_defaults)
                    .is_empty();
            let previous_env = snapshot.env.clone();
            snapshot.env = compatibility_env(&definitions, &snapshot.values, |definition| {
                definition.scope == "shared"
                    || definition.service_name.as_deref() == Some(snapshot.service_name.as_str())
            });
            if changed || snapshot.env != previous_env {
                snapshot.checksum = checksum(&json!({
                    "values": snapshot.values,
                    "env": snapshot.env,
                }))?;
                self.store.save_snapshot(&snapshot).await?;
            }
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "add Local Connector runtime configuration",
        )
        .await?;

        tracing::info!(
            user_service_base_url_key = LOCAL_CONNECTOR_USER_SERVICE_BASE_URL_CONFIG_KEY,
            public_base_url_key = LOCAL_CONNECTOR_PUBLIC_BASE_URL_CONFIG_KEY,
            relay_timeout_key = LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "Local Connector runtime configuration is present in releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_memory_engine_runtime_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = memory_engine_runtime_default_values(&definitions);
        if defaults.len() != MEMORY_ENGINE_RUNTIME_CONFIG_KEYS.len() {
            return Err(
                "memory engine runtime configuration definitions are incomplete".to_string(),
            );
        }
        let values_by_release = self
            .migrate_release_default_values(&defaults, |values, defaults| {
                Ok(ensure_memory_engine_runtime_values(values, defaults))
            })
            .await?;

        for mut snapshot in self.store.list_all_snapshots().await? {
            if snapshot.service_name != "memory-engine" {
                continue;
            }
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_memory_engine_runtime_values(&mut snapshot.values, &snapshot_defaults)
                    .is_empty();
            let previous_env = snapshot.env.clone();
            snapshot.env = compatibility_env(&definitions, &snapshot.values, |definition| {
                definition.scope == "shared"
                    || definition.service_name.as_deref() == Some(snapshot.service_name.as_str())
            });
            if changed || snapshot.env != previous_env {
                snapshot.checksum = checksum(&json!({
                    "values": snapshot.values,
                    "env": snapshot.env,
                }))?;
                self.store.save_snapshot(&snapshot).await?;
            }
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "add Memory Engine runtime configuration",
        )
        .await?;

        tracing::info!(
            user_service_base_url_key = MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY,
            user_service_internal_base_url_key = MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
            user_service_timeout_key = MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "Memory Engine runtime configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }
}
