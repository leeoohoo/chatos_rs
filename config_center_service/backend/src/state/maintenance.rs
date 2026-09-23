// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::catalog::{
    DEFAULT_LOCAL_RABBITMQ_URL, TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY,
};

#[path = "maintenance_services.rs"]
mod maintenance_services;

impl AppState {
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

    pub(super) async fn migrate_agent_max_iterations_config(&self) -> Result<(), String> {
        use chatos_agent::{AGENT_MAX_ITERATIONS_CONFIG_KEY, DEFAULT_AGENT_MAX_ITERATIONS};

        let mut values_by_release = BTreeMap::new();
        for mut release in self.store.list_all_releases().await? {
            let changed = migrate_agent_iteration_values(&mut release.values, true);
            let keys_changed = migrate_agent_iteration_changed_keys(&mut release.changed_keys);
            values_by_release.insert(
                (release.environment.clone(), release.revision),
                release
                    .values
                    .get(AGENT_MAX_ITERATIONS_CONFIG_KEY)
                    .cloned()
                    .unwrap_or_else(|| json!(DEFAULT_AGENT_MAX_ITERATIONS)),
            );
            if changed || keys_changed {
                self.store.save_release(&release).await?;
            }
        }

        let definitions = self.store.list_definitions().await?;
        for mut snapshot in self.store.list_all_snapshots().await? {
            let fallback = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| json!(DEFAULT_AGENT_MAX_ITERATIONS));
            let changed =
                migrate_agent_iteration_values_with_fallback(&mut snapshot.values, fallback, true);
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

        for mut draft in self.store.list_drafts().await? {
            if migrate_agent_iteration_values(&mut draft.changes, false) {
                draft.validation_status = "pending".to_string();
                draft.validation_errors.clear();
                draft.updated_at = Utc::now().to_rfc3339();
                self.store.save_draft(&draft).await?;
            }
        }

        for mut event in self.store.list_all_audit().await? {
            if migrate_agent_iteration_changed_keys(&mut event.changed_keys) {
                self.store.save_audit(&event).await?;
            }
        }

        self.republish_active_releases_to_consul(&definitions, "consolidate Agent configuration")
            .await?;

        tracing::info!(
            key = AGENT_MAX_ITERATIONS_CONFIG_KEY,
            "Agent max-iterations configuration is consolidated in configuration center"
        );
        Ok(())
    }

    pub(super) async fn migrate_task_runner_runtime_config(&self) -> Result<(), String> {
        use chatos_agent::{AGENT_MAX_ITERATIONS_CONFIG_KEY, DEFAULT_AGENT_MAX_ITERATIONS};

        let definitions = self.store.list_definitions().await?;
        let task_runner_defaults = task_runner_service_default_values(&definitions);
        let mut values_by_release = BTreeMap::new();
        for mut release in self.store.list_all_releases().await? {
            let mut release_defaults = task_runner_defaults.clone();
            let selected_max_iterations = release
                .values
                .get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY)
                .cloned()
                .or_else(|| release.values.get(AGENT_MAX_ITERATIONS_CONFIG_KEY).cloned())
                .unwrap_or_else(|| json!(DEFAULT_AGENT_MAX_ITERATIONS));
            release_defaults.insert(
                TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY.to_string(),
                selected_max_iterations,
            );
            let changed_keys =
                ensure_task_runner_runtime_values(&mut release.values, &release_defaults);
            values_by_release.insert(
                (release.environment.clone(), release.revision),
                release.values.clone(),
            );
            if !changed_keys.is_empty() {
                for key in changed_keys {
                    ensure_changed_key(&mut release.changed_keys, key.as_str());
                }
                self.store.save_release(&release).await?;
            }
        }

        for snapshot in self.store.list_all_snapshots().await? {
            let all_values = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .ok_or_else(|| {
                    format!(
                        "release values are unavailable for snapshot {}/{} revision {}",
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

        for mut draft in self.store.list_drafts().await? {
            let changed = migrate_task_runner_queue_mode_draft(
                &mut draft.changes,
                TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY,
            ) | migrate_task_runner_queue_mode_draft(
                &mut draft.changes,
                TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY,
            ) | ensure_root_vhost_rabbitmq_url(
                &mut draft.changes,
                TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
                task_runner_defaults
                    .get(TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY)
                    .unwrap_or(&json!(DEFAULT_LOCAL_RABBITMQ_URL)),
            ) | migrate_https_url_draft(
                &mut draft.changes,
                TASK_RUNNER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
                task_runner_defaults
                    .get(TASK_RUNNER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY)
                    .ok_or_else(|| {
                        "Task Runner Memory Engine HTTPS default is missing".to_string()
                    })?,
            ) | migrate_https_url_draft(
                &mut draft.changes,
                TASK_RUNNER_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
                task_runner_defaults
                    .get(TASK_RUNNER_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY)
                    .ok_or_else(|| {
                        "Task Runner User Service HTTPS default is missing".to_string()
                    })?,
            );
            if changed {
                draft.validation_status = "pending".to_string();
                draft.validation_errors.clear();
                draft.updated_at = Utc::now().to_rfc3339();
                self.store.save_draft(&draft).await?;
            }
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "add Task Runner runtime configuration",
        )
        .await?;

        tracing::info!(
            key = TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY,
            fallback_key = AGENT_MAX_ITERATIONS_CONFIG_KEY,
            callback_delivery_mode_key = TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY,
            "Task Runner runtime configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_mcp_management_runtime_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = mcp_management_service_default_values(&definitions);
        if defaults.len() != MCP_MANAGEMENT_RUNTIME_CONFIG_KEYS.len() {
            return Err(
                "MCP Management runtime configuration definitions are incomplete".to_string(),
            );
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys = ensure_mcp_management_runtime_values(&mut release.values, &defaults);
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

        for mut snapshot in self.store.list_all_snapshots().await? {
            if snapshot.service_name != "mcp-management-service" {
                continue;
            }
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_mcp_management_runtime_values(&mut snapshot.values, &snapshot_defaults)
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

        for mut draft in self.store.list_drafts().await? {
            if ensure_root_vhost_rabbitmq_url(
                &mut draft.changes,
                MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY,
                defaults
                    .get(MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY)
                    .unwrap_or(&json!(DEFAULT_LOCAL_RABBITMQ_URL)),
            ) | migrate_https_url_draft(
                &mut draft.changes,
                MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY,
                defaults
                    .get(MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY)
                    .ok_or_else(|| {
                        "MCP Management Plugin Management HTTPS default is missing".to_string()
                    })?,
            ) {
                draft.validation_status = "pending".to_string();
                draft.validation_errors.clear();
                draft.updated_at = Utc::now().to_rfc3339();
                self.store.save_draft(&draft).await?;
            }
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "add MCP Management runtime configuration",
        )
        .await?;

        tracing::info!(
            dispatch_mode_key = MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY,
            configuration_center_secret_key =
                MCP_MANAGEMENT_CONFIGURATION_CENTER_INTERNAL_API_SECRET_CONFIG_KEY,
            allowed_callers_key = MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS_CONFIG_KEY,
            rabbitmq_url_key = MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY,
            dispatch_queue_key = MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
            "MCP Management runtime configuration is present in configuration center releases and snapshots"
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
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys =
                ensure_local_connector_runtime_values(&mut release.values, &defaults);
            values_by_release.insert(
                (release.environment.clone(), release.revision),
                defaults
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
                    .collect::<BTreeMap<_, _>>(),
            );
            if !changed_keys.is_empty() {
                for key in changed_keys {
                    ensure_changed_key(&mut release.changed_keys, key.as_str());
                }
                self.store.save_release(&release).await?;
            }
        }

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
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys = ensure_memory_engine_runtime_values(&mut release.values, &defaults);
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
