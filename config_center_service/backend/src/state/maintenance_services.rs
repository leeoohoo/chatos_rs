// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
impl AppState {
    pub(in crate::state) async fn migrate_platform_pressure_config(&self) -> Result<(), String> {
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

    pub(in crate::state) async fn migrate_internal_request_security_config(
        &self,
    ) -> Result<(), String> {
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
            memory_engine_key = MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "Internal request security configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(in crate::state) async fn migrate_user_service_smtp_config(&self) -> Result<(), String> {
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

    pub(in crate::state) async fn migrate_user_service_runtime_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = user_service_runtime_default_values(&definitions);
        if defaults.len() != USER_SERVICE_RUNTIME_CONFIG_KEYS.len() {
            return Err(
                "user service runtime configuration definitions are incomplete".to_string(),
            );
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys = ensure_user_service_startup_values(&mut release.values, &defaults)?;
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

    pub(in crate::state) async fn migrate_plugin_management_runtime_config(
        &self,
    ) -> Result<(), String> {
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

    pub(in crate::state) async fn migrate_chatos_ui_config(&self) -> Result<(), String> {
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
            task_runner_base_url_key = CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            memory_engine_base_url_key = CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            "ChatOS runtime configuration is present in releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn republish_active_releases_to_consul(
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
