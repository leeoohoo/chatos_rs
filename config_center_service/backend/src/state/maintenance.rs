// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
            let release_values = release_defaults
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
                release_values,
            );
            if !changed_keys.is_empty() {
                for key in changed_keys {
                    ensure_changed_key(&mut release.changed_keys, key.as_str());
                }
                self.store.save_release(&release).await?;
            }
        }

        for mut snapshot in self.store.list_all_snapshots().await? {
            if snapshot.service_name != "task-runner" {
                continue;
            }
            let mut snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| task_runner_defaults.clone());
            if !snapshot
                .values
                .contains_key(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY)
            {
                if let Some(shared_max_iterations) = snapshot
                    .values
                    .get(AGENT_MAX_ITERATIONS_CONFIG_KEY)
                    .cloned()
                {
                    snapshot_defaults.insert(
                        TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY.to_string(),
                        shared_max_iterations,
                    );
                }
            }
            let changed =
                !ensure_task_runner_runtime_values(&mut snapshot.values, &snapshot_defaults)
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
            "add Task Runner runtime configuration",
        )
        .await?;

        tracing::info!(
            key = TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY,
            fallback_key = AGENT_MAX_ITERATIONS_CONFIG_KEY,
            environment_mode_key = TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY,
            "Task Runner runtime configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_mcp_management_runtime_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = mcp_management_service_default_values(&definitions);
        if defaults.len() != 16 {
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

        self.republish_active_releases_to_consul(
            &definitions,
            "add MCP Management runtime configuration",
        )
        .await?;

        tracing::info!(
            dispatch_mode_key = MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY,
            internal_secret_key = MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            allowed_callers_key = MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS_CONFIG_KEY,
            rabbitmq_url_key = MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY,
            dispatch_queue_key = MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
            "MCP Management runtime configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_sandbox_manager_pool_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = sandbox_manager_pool_default_values(&definitions);
        if defaults.len() != 2 {
            return Err(
                "Sandbox Manager pool configuration definitions are incomplete".to_string(),
            );
        }
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed_keys = ensure_sandbox_manager_pool_values(&mut release.values, &defaults);
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
            if snapshot.service_name != "sandbox-manager" {
                continue;
            }
            let snapshot_defaults = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| defaults.clone());
            let changed =
                !ensure_sandbox_manager_pool_values(&mut snapshot.values, &snapshot_defaults)
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
            "add Sandbox Manager pool configuration",
        )
        .await?;

        tracing::info!(
            max_active_key = SANDBOX_MANAGER_POOL_MAX_ACTIVE_CONFIG_KEY,
            max_pending_key = SANDBOX_MANAGER_POOL_MAX_PENDING_CONFIG_KEY,
            "Sandbox Manager pool configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_internal_request_security_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = internal_request_security_default_values(&definitions);
        if defaults.len() != 5 {
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
                "project-service",
                "memory-engine",
                "sandbox-manager",
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

        self.republish_active_releases_to_consul(
            &definitions,
            "add internal request security configuration",
        )
        .await?;

        tracing::info!(
            local_connector_key = LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            mcp_management_key = MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            project_service_key = PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            memory_engine_key = MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            sandbox_manager_key = SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "Internal request security configuration is present in configuration center releases and snapshots"
        );
        Ok(())
    }

    pub(super) async fn migrate_user_service_smtp_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let defaults = user_service_smtp_default_values(&definitions);
        if defaults.len() != 5 {
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

    pub(super) async fn migrate_chatos_ui_config(&self) -> Result<(), String> {
        let definitions = self.store.list_definitions().await?;
        let default_value = chatos_local_project_creation_default_value(&definitions)
            .ok_or_else(|| {
                format!(
                    "missing ChatOS configuration definition for {CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY}"
                )
            })?;
        let mut values_by_release = BTreeMap::new();

        for mut release in self.store.list_all_releases().await? {
            let changed = ensure_chatos_local_project_creation_value(
                &mut release.values,
                default_value.clone(),
            );
            values_by_release.insert(
                (release.environment.clone(), release.revision),
                release
                    .values
                    .get(CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY)
                    .cloned()
                    .unwrap_or_else(|| default_value.clone()),
            );
            if changed {
                ensure_changed_key(
                    &mut release.changed_keys,
                    CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY,
                );
                self.store.save_release(&release).await?;
            }
        }

        for mut snapshot in self.store.list_all_snapshots().await? {
            if snapshot.service_name != "chatos-backend" {
                continue;
            }
            let fallback = values_by_release
                .get(&(snapshot.environment.clone(), snapshot.revision))
                .cloned()
                .unwrap_or_else(|| default_value.clone());
            let changed =
                ensure_chatos_local_project_creation_value(&mut snapshot.values, fallback);
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

        self.republish_active_releases_to_consul(&definitions, "add ChatOS UI configuration")
            .await?;

        tracing::info!(
            key = CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY,
            "ChatOS local-project entry configuration is present in releases and snapshots"
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
