// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::catalog::{
    DEFAULT_LOCAL_RABBITMQ_URL, TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY,
};

impl AppState {
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
                TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
                task_runner_defaults
                    .get(TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY)
                    .ok_or_else(|| {
                        "Task Runner Project Service HTTPS default is missing".to_string()
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
                MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL_CONFIG_KEY,
                defaults
                    .get(MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL_CONFIG_KEY)
                    .ok_or_else(|| {
                        "MCP Management Project Service HTTPS default is missing".to_string()
                    })?,
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
            user_service_timeout_key = MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "Memory Engine runtime configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_platform_pressure_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = platform_pressure_default_values(&definitions);
        if defaults.len() != PLATFORM_PRESSURE_CONFIG_KEYS.len() {
            return Err("platform pressure configuration definitions are incomplete".to_string());
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys = ensure_platform_pressure_values(&mut release.values, &defaults);
            values_by_release.insert(
                (release.environment.clone(), release.revision),
                defaults
                    .iter()
                    .map(|(key, default)| {
                        (
                            key.clone(),
                            release
                                .values
                                .get(key)
                                .cloned()
                                .unwrap_or_else(|| default.clone()),
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
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_platform_pressure_values(&mut snapshot.values, &snapshot_defaults)
                    .is_empty();
            if changed {
                snapshot.checksum = checksum(&json!({
                    "values": snapshot.values,
                    "env": snapshot.env,
                }))?;
                self.store.save_snapshot(&snapshot).await?;
            }
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "add authoritative platform pressure state",
        )
        .await?;

        tracing::info!(
            pressure_level_key = PLATFORM_PRESSURE_LEVEL_CONFIG_KEY,
            "Platform pressure state is present in all configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_internal_request_security_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = internal_request_security_default_values(&definitions);
        if defaults.len() != INTERNAL_REQUEST_SECURITY_CONFIG_KEYS.len() {
            return Err(
                "internal request security configuration definitions are incomplete".to_string(),
            );
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys =
                ensure_internal_request_security_values(&mut release.values, &defaults);
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
            if ![
                "local-connector-service",
                "mcp-management-service",
                "plugin-management-service",
                "project-service",
                "memory-engine",
                "task-runner",
                "chatos-backend",
                "user-service",
            ]
            .contains(&snapshot.service_name.as_str())
            {
                continue;
            }
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_internal_request_security_values(&mut snapshot.values, &snapshot_defaults)
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
            let Some(value) = draft
                .changes
                .get(CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY)
            else {
                continue;
            };
            if value
                .as_str()
                .is_some_and(|value| value.trim().starts_with("https://"))
            {
                continue;
            }
            let replacement = defaults
                .get(CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY)
                .cloned()
                .ok_or_else(|| {
                    "Configuration Center Memory Engine HTTPS default is missing".to_string()
                })?;
            draft.changes.insert(
                CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY.to_string(),
                replacement,
            );
            draft.validation_status = "pending".to_string();
            draft.validation_errors.clear();
            draft.updated_at = Utc::now().to_rfc3339();
            self.store.save_draft(&draft).await?;
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "add internal request security configuration",
        )
        .await?;

        tracing::info!(
            local_connector_key = LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            mcp_management_key = MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            plugin_management_key = PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            project_service_key = PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            memory_engine_key = MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "Internal request security configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_user_service_smtp_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = user_service_smtp_default_values(&definitions);
        if defaults.len() != 6 {
            return Err("user service SMTP configuration definitions are incomplete".to_string());
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys = ensure_user_service_smtp_values(&mut release.values, &defaults);
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
            if snapshot.service_name != "user-service" {
                continue;
            }
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_user_service_smtp_values(&mut snapshot.values, &snapshot_defaults)
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
            "add User Service SMTP configuration",
        )
        .await?;

        tracing::info!(
            smtp_host_key = USER_SERVICE_SMTP_HOST_CONFIG_KEY,
            smtp_port_key = USER_SERVICE_SMTP_PORT_CONFIG_KEY,
            smtp_username_key = USER_SERVICE_SMTP_USERNAME_CONFIG_KEY,
            email_from_key = USER_SERVICE_EMAIL_FROM_CONFIG_KEY,
            email_from_name_key = USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY,
            "User Service SMTP configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_user_service_runtime_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = user_service_runtime_default_values(&definitions);
        if defaults.len() != 27 {
            return Err(
                "user service runtime configuration definitions are incomplete".to_string(),
            );
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys = ensure_user_service_runtime_values(&mut release.values, &defaults);
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
            if snapshot.service_name != "user-service" {
                continue;
            }
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_user_service_runtime_values(&mut snapshot.values, &snapshot_defaults)
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
            "add User Service runtime configuration",
        )
        .await?;

        tracing::info!(
            internal_mtls_port_key = USER_SERVICE_INTERNAL_MTLS_PORT_CONFIG_KEY,
            task_runner_base_url_key = USER_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            harness_enabled_key = USER_SERVICE_HARNESS_PROVISIONING_ENABLED_CONFIG_KEY,
            "User Service runtime configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_project_service_runtime_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = project_service_runtime_default_values(&definitions);
        if defaults.is_empty() {
            return Err(
                "Project Service runtime configuration definitions are incomplete".to_string(),
            );
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys =
                ensure_project_service_runtime_values(&mut release.values, &defaults);
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
            if snapshot.service_name != "project-service" {
                continue;
            }
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_project_service_runtime_values(&mut snapshot.values, &snapshot_defaults)
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
            let mut changed = false;
            {
                let key = PROJECT_SERVICE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY;
                let replacement = defaults
                    .get(key)
                    .ok_or_else(|| format!("Project Service HTTPS default is missing: {key}"))?;
                changed |= migrate_https_url_draft(&mut draft.changes, key, replacement);
            }
            if changed {
                draft.validation_status = "pending".to_string();
                draft.validation_errors.clear();
                draft.updated_at = Utc::now().to_rfc3339();
                self.store.save_draft(&draft).await?;
            }
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "add Project Service runtime configuration",
        )
        .await?;

        tracing::info!(
            user_service_base_url_key = PROJECT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY,
            user_service_internal_base_url_key =
                PROJECT_SERVICE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
            local_connector_base_url_key =
                PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
            task_runner_base_url_key = PROJECT_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            "Project Service runtime configuration is present in releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_plugin_management_runtime_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = plugin_management_service_runtime_default_values(&definitions);
        if defaults.is_empty() {
            return Err(
                "Plugin Management runtime configuration definitions are incomplete".to_string(),
            );
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys =
                ensure_plugin_management_runtime_values(&mut release.values, &defaults);
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
            let release_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let snapshot_defaults = plugin_management_snapshot_default_values(
                &release_defaults,
                snapshot.service_name.as_str(),
            );
            let changed =
                !ensure_plugin_management_runtime_values(&mut snapshot.values, &snapshot_defaults)
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
            let replacement = defaults
                .get(SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY)
                .ok_or_else(|| "Plugin Management internal HTTPS default is missing".to_string())?;
            if migrate_https_url_draft(
                &mut draft.changes,
                SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY,
                replacement,
            ) {
                draft.validation_status = "pending".to_string();
                draft.validation_errors.clear();
                draft.updated_at = Utc::now().to_rfc3339();
                self.store.save_draft(&draft).await?;
            }
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "add Plugin Management runtime configuration",
        )
        .await?;

        tracing::info!(
            user_service_base_url_key = PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY,
            task_runner_base_url_key = PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            catalog_request_timeout_key = PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            shared_service_url_key = SHARED_PLUGIN_MANAGEMENT_SERVICE_URL_CONFIG_KEY,
            shared_internal_service_url_key =
                SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY,
            shared_request_timeout_key = SHARED_PLUGIN_MANAGEMENT_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "Plugin Management runtime configuration is present in releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_chatos_ui_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = chatos_service_default_values(&definitions);
        if defaults.is_empty() {
            return Err("ChatOS runtime configuration definitions are incomplete".to_string());
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys = ensure_chatos_runtime_values(&mut release.values, &defaults);
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
            if snapshot.service_name != "chatos-backend" {
                continue;
            }
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_chatos_runtime_values(&mut snapshot.values, &snapshot_defaults).is_empty();
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
            let replacement = defaults
                .get(CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY)
                .ok_or_else(|| "ChatOS Memory Engine HTTPS default is missing".to_string())?;
            if migrate_https_url_draft(
                &mut draft.changes,
                CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
                replacement,
            ) | migrate_service_url_draft(
                &mut draft.changes,
                CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
                defaults
                    .get(CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY)
                    .ok_or_else(|| {
                        "ChatOS User Service internal HTTPS default is missing".to_string()
                    })?,
                &["https://127.0.0.1:39192", "https://localhost:39192"],
            ) | migrate_https_url_draft(
                &mut draft.changes,
                CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
                defaults
                    .get(CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY)
                    .ok_or_else(|| "ChatOS Project Service HTTPS default is missing".to_string())?,
            ) {
                draft.validation_status = "pending".to_string();
                draft.validation_errors.clear();
                draft.updated_at = Utc::now().to_rfc3339();
                self.store.save_draft(&draft).await?;
            }
        }

        self.republish_active_releases_to_consul(
            &definitions,
            "refresh ChatOS runtime configuration",
        )
        .await?;

        tracing::info!(
            user_service_base_url_key = CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY,
            user_service_internal_base_url_key = CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
            project_service_base_url_key = CHATOS_PROJECT_SERVICE_BASE_URL_CONFIG_KEY,
            task_runner_base_url_key = CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            memory_engine_base_url_key = CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            "ChatOS runtime configuration is present in releases and snapshots"
        );
        Ok(())
    }

    async fn republish_active_releases_to_consul(
        &self,
        definitions: &[ConfigDefinitionRecord],
        maintenance_action: &str,
    ) -> Result<(), String> {
        for active in self.store.list_active_releases().await? {
            let Some(release) = self.store.get_release(active.release_id.as_str()).await? else {
                continue;
            };
            if let Err(err) = self
                .publish_consul(
                    active.environment.as_str(),
                    active.revision,
                    definitions,
                    &release.values,
                )
                .await
            {
                if self.config.consul_required {
                    return Err(err);
                }
                tracing::warn!(
                    environment = active.environment.as_str(),
                    maintenance_action,
                    error = err.as_str(),
                    "failed to republish Consul after configuration maintenance"
                );
            }
        }
        Ok(())
    }
}
