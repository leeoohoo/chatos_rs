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
