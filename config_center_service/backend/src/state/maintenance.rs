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
        for active in self.store.list_active_releases().await? {
            let Some(release) = self.store.get_release(active.release_id.as_str()).await? else {
                continue;
            };
            if let Err(err) = self
                .publish_consul(
                    active.environment.as_str(),
                    active.revision,
                    &definitions,
                    &release.values,
                )
                .await
            {
                if self.config.consul_required {
                    return Err(err);
                }
                tracing::warn!(
                    environment = active.environment.as_str(),
                    error = err.as_str(),
                    "failed to republish Consul after removing user preferences"
                );
            }
        }
        Ok(())
    }

    pub(super) async fn migrate_agent_max_iterations_config(&self) -> Result<(), String> {
        use chatos_agent::{
            AGENT_MAX_ITERATIONS_CONFIG_KEY, AGENT_MAX_ITERATIONS_ENV,
            DEFAULT_AGENT_MAX_ITERATIONS, LEGACY_CHATOS_MAX_ITERATIONS_ENV,
            LEGACY_TASK_RUNNER_MAX_ITERATIONS_ENV,
        };

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

        for active in self.store.list_active_releases().await? {
            let Some(release) = self.store.get_release(active.release_id.as_str()).await? else {
                continue;
            };
            if let Err(err) = self
                .publish_consul(
                    active.environment.as_str(),
                    active.revision,
                    &definitions,
                    &release.values,
                )
                .await
            {
                if self.config.consul_required {
                    return Err(err);
                }
                tracing::warn!(
                    environment = active.environment.as_str(),
                    error = err.as_str(),
                    "failed to republish Consul after consolidating Agent configuration"
                );
            }
        }

        tracing::info!(
            key = AGENT_MAX_ITERATIONS_CONFIG_KEY,
            env = AGENT_MAX_ITERATIONS_ENV,
            legacy_chatos_env = LEGACY_CHATOS_MAX_ITERATIONS_ENV,
            legacy_task_runner_env = LEGACY_TASK_RUNNER_MAX_ITERATIONS_ENV,
            "Agent max-iterations configuration is consolidated"
        );
        Ok(())
    }
}
