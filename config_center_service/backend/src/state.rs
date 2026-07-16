// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use mongodb::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use chatos_config_sdk::ConfigSnapshot;

use crate::catalog::{
    builtin_definitions, LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS, USER_PREFERENCE_CONFIG_KEYS,
};
use crate::config::AppConfig;
use crate::models::{
    ActiveReleaseRecord, AuditEventRecord, ConfigDefinitionRecord, ConfigDraftRecord,
    ConfigReleaseRecord, CurrentUser, CustomDefinitionRequest, EffectiveConfigResponse,
    ServiceInstanceRecord, ValidationResponse,
};
use crate::store::AppStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: AppStore,
    http: reqwest::Client,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let client = Client::with_uri_str(config.database_url.as_str())
            .await
            .map_err(|err| format!("connect configuration center MongoDB failed: {err}"))?;
        let store = AppStore::new(client.database(config.mongodb_database.as_str()));
        store.initialize().await?;
        store
            .delete_definitions(USER_PREFERENCE_CONFIG_KEYS)
            .await?;
        store
            .delete_definitions(LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS)
            .await?;
        for definition in builtin_definitions() {
            store.upsert_definition(&definition).await?;
        }
        let state = Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .map_err(|err| err.to_string())?,
            config,
            store,
        };
        state
            .ensure_initial_release(state.config.default_environment.as_str())
            .await?;
        state.purge_user_preferences_from_config_center().await?;
        state.migrate_agent_max_iterations_config().await?;
        Ok(state)
    }

    pub async fn ensure_initial_release(&self, environment: &str) -> Result<(), String> {
        if self.store.get_active(environment).await?.is_some() {
            return Ok(());
        }
        let values = self.default_values().await?;
        self.publish_values(
            environment,
            values,
            &system_user(),
            "Initialize configuration catalog defaults",
            Vec::new(),
        )
        .await
        .map(|_| ())
    }

    pub async fn create_custom_definition(
        &self,
        input: CustomDefinitionRequest,
        user: &CurrentUser,
    ) -> Result<ConfigDefinitionRecord, String> {
        let key = input.key.trim().to_ascii_lowercase();
        if key.is_empty()
            || key.len() > 160
            || !key.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err(
                "Custom key must use lowercase letters, digits, dots, underscores or dashes"
                    .to_string(),
            );
        }
        if !key.starts_with("developer.") && !key.contains('.') {
            return Err(
                "Custom key must use a namespaced key such as developer.feature.name".to_string(),
            );
        }
        if USER_PREFERENCE_CONFIG_KEYS.contains(&key.as_str()) {
            return Err("This key is reserved for Chat OS user preferences".to_string());
        }
        if !matches!(
            input.value_type.as_str(),
            "string" | "integer" | "boolean" | "duration_ms" | "bytes" | "enum" | "json"
        ) {
            return Err("Unsupported custom value type".to_string());
        }
        if !matches!(
            input.reload_mode.as_str(),
            "hot_reload" | "next_request" | "next_run" | "restart_required"
        ) {
            return Err("Unsupported reload mode".to_string());
        }
        if input.scope != "shared"
            && input
                .service_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
        {
            return Err("Service-scoped custom configuration requires service_name".to_string());
        }
        let now = Utc::now().to_rfc3339();
        let definition = ConfigDefinitionRecord {
            id: key.clone(),
            key: key.clone(),
            display_name: input.display_name.trim().to_string(),
            description: input.description.unwrap_or_default().trim().to_string(),
            category: input
                .category
                .unwrap_or_else(|| "Developer".to_string())
                .trim()
                .to_string(),
            scope: input.scope,
            service_name: input
                .service_name
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            value_type: input.value_type,
            default_value: input.default_value.clone(),
            nullable: false,
            min: input.min,
            max: input.max,
            enum_options: input.enum_options,
            sensitivity: "public".to_string(),
            reload_mode: input.reload_mode,
            criticality: "normal".to_string(),
            env_aliases: input
                .env_aliases
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| {
                    !value.is_empty()
                        && value.bytes().all(|byte| {
                            byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
                        })
                })
                .collect(),
            owner_team: "platform".to_string(),
            ui_order: 10_000,
            deprecated: false,
            created_at: now.clone(),
            updated_at: now,
        };
        let mut errors = Vec::new();
        validate_definition(&definition, &definition.default_value, &mut errors);
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        self.store.upsert_definition(&definition).await?;
        let mut changes = self
            .store
            .get_draft(input.environment.as_str())
            .await?
            .map(|draft| draft.changes)
            .unwrap_or_default();
        changes.insert(key.clone(), input.default_value);
        self.save_draft(input.environment.as_str(), changes, user)
            .await?;
        self.audit(
            Some(input.environment.as_str()),
            "catalog.custom_created",
            user,
            None,
            vec![key],
            None,
        )
        .await?;
        Ok(definition)
    }

    pub async fn effective(&self, environment: &str) -> Result<EffectiveConfigResponse, String> {
        let release = self.store.get_active_release(environment).await?;
        Ok(EffectiveConfigResponse {
            environment: environment.to_string(),
            revision: release
                .as_ref()
                .map(|item| item.revision)
                .unwrap_or_default(),
            release_id: release.as_ref().map(|item| item.id.clone()),
            values: match release {
                Some(release) => release.values,
                None => self.default_values().await?,
            },
        })
    }

    pub async fn save_draft(
        &self,
        environment: &str,
        changes: BTreeMap<String, Value>,
        user: &CurrentUser,
    ) -> Result<ConfigDraftRecord, String> {
        let active = self.store.get_active(environment).await?;
        let now = Utc::now().to_rfc3339();
        let existing = self.store.get_draft(environment).await?;
        let draft = ConfigDraftRecord {
            id: existing
                .as_ref()
                .map(|item| item.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            environment: environment.to_string(),
            base_revision: active
                .as_ref()
                .map(|item| item.revision)
                .unwrap_or_default(),
            changes,
            validation_status: "pending".to_string(),
            validation_errors: Vec::new(),
            updated_by: user.user_id.clone(),
            created_at: existing
                .map(|item| item.created_at)
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        self.store.save_draft(&draft).await?;
        self.audit(
            Some(environment),
            "draft.updated",
            user,
            None,
            draft.changes.keys().cloned().collect(),
            None,
        )
        .await?;
        Ok(draft)
    }

    pub async fn validate_draft(&self, environment: &str) -> Result<ValidationResponse, String> {
        let Some(mut draft) = self.store.get_draft(environment).await? else {
            return Ok(ValidationResponse {
                valid: false,
                errors: vec!["No active draft".to_string()],
            });
        };
        let values = self
            .values_with_changes(environment, &draft.changes)
            .await?;
        let errors = self.validate_values(&values).await?;
        draft.validation_status = if errors.is_empty() {
            "valid".to_string()
        } else {
            "invalid".to_string()
        };
        draft.validation_errors = errors.clone();
        draft.updated_at = Utc::now().to_rfc3339();
        self.store.save_draft(&draft).await?;
        Ok(ValidationResponse {
            valid: errors.is_empty(),
            errors,
        })
    }

    pub async fn publish_draft(
        &self,
        environment: &str,
        user: &CurrentUser,
        message: &str,
    ) -> Result<ConfigReleaseRecord, String> {
        let draft = self
            .store
            .get_draft(environment)
            .await?
            .ok_or_else(|| "No active draft".to_string())?;
        let active = self.store.get_active(environment).await?;
        let active_revision = active
            .as_ref()
            .map(|item| item.revision)
            .unwrap_or_default();
        if draft.base_revision != active_revision {
            return Err(format!(
                "Draft is based on revision {}, but active revision is {}",
                draft.base_revision, active_revision
            ));
        }
        let values = self
            .values_with_changes(environment, &draft.changes)
            .await?;
        let errors = self.validate_values(&values).await?;
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        let changed_keys = draft.changes.keys().cloned().collect();
        let release = self
            .publish_values(environment, values, user, message, changed_keys)
            .await?;
        self.store.delete_draft(environment).await?;
        Ok(release)
    }

    pub async fn rollback(
        &self,
        environment: &str,
        release_id: &str,
        user: &CurrentUser,
    ) -> Result<ConfigReleaseRecord, String> {
        let source = self
            .store
            .get_release(release_id)
            .await?
            .ok_or_else(|| "Release not found".to_string())?;
        if source.environment != environment {
            return Err("Release environment does not match".to_string());
        }
        let current = self.effective(environment).await?;
        let changed_keys = changed_keys(&current.values, &source.values);
        self.publish_values(
            environment,
            source.values,
            user,
            format!("Rollback to revision {}", source.revision).as_str(),
            changed_keys,
        )
        .await
    }

    pub async fn snapshot(
        &self,
        environment: &str,
        service_name: &str,
    ) -> Result<ConfigSnapshot, String> {
        self.store
            .get_active_snapshot(environment, service_name)
            .await?
            .ok_or_else(|| format!("No published snapshot for {environment}/{service_name}"))
    }

    async fn publish_values(
        &self,
        environment: &str,
        values: BTreeMap<String, Value>,
        user: &CurrentUser,
        message: &str,
        changed_keys: Vec<String>,
    ) -> Result<ConfigReleaseRecord, String> {
        let definitions = self.store.list_definitions().await?;
        let active = self.store.get_active(environment).await?;
        let revision = self.store.next_release_revision(environment).await?;
        let now = Utc::now().to_rfc3339();
        let mut release = ConfigReleaseRecord {
            id: Uuid::new_v4().to_string(),
            environment: environment.to_string(),
            revision,
            status: "building".to_string(),
            base_release_id: active.as_ref().map(|item| item.release_id.clone()),
            changed_keys: changed_keys.clone(),
            values: values.clone(),
            publish_message: message.trim().to_string(),
            created_by: user.user_id.clone(),
            created_at: now.clone(),
            published_at: None,
            error: None,
        };
        self.store.insert_release(&release).await?;

        let services = known_services(&definitions);
        let mut snapshots = Vec::new();
        for service_name in services {
            let snapshot = build_snapshot(
                environment,
                service_name.as_str(),
                revision,
                &definitions,
                &values,
            )?;
            self.store.insert_snapshot(&snapshot).await?;
            snapshots.push(snapshot);
        }

        if let Err(err) = self
            .publish_consul(environment, revision, &definitions, &values)
            .await
        {
            release.status = "failed".to_string();
            release.error = Some(err.clone());
            self.store.save_release(&release).await?;
            if self.config.consul_required {
                return Err(err);
            }
            tracing::warn!(
                environment,
                revision,
                error = err.as_str(),
                "Consul publish failed; continuing because Consul is optional"
            );
        }

        self.store
            .set_active(&ActiveReleaseRecord {
                id: environment.to_string(),
                environment: environment.to_string(),
                release_id: release.id.clone(),
                revision,
                updated_at: now.clone(),
            })
            .await?;
        release.status = "published".to_string();
        release.published_at = Some(now);
        release.error = None;
        self.store.save_release(&release).await?;
        self.audit(
            Some(environment),
            "release.published",
            user,
            Some(release.id.as_str()),
            changed_keys,
            Some(json!({ "revision": revision, "snapshot_count": snapshots.len() })),
        )
        .await?;
        Ok(release)
    }

    async fn values_with_changes(
        &self,
        environment: &str,
        changes: &BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>, String> {
        let mut values = self.effective(environment).await?.values;
        let definitions = self.store.list_definitions().await?;
        let known = definitions
            .iter()
            .map(|definition| definition.key.as_str())
            .collect::<BTreeSet<_>>();
        for (key, value) in changes {
            if !known.contains(key.as_str()) {
                return Err(format!("Unknown configuration key: {key}"));
            }
            if value.is_null() {
                let default = definitions
                    .iter()
                    .find(|definition| definition.key == *key)
                    .map(|definition| definition.default_value.clone())
                    .unwrap_or(Value::Null);
                values.insert(key.clone(), default);
            } else {
                values.insert(key.clone(), value.clone());
            }
        }
        Ok(values)
    }

    async fn default_values(&self) -> Result<BTreeMap<String, Value>, String> {
        Ok(self
            .store
            .list_definitions()
            .await?
            .into_iter()
            .map(|definition| (definition.key, definition.default_value))
            .collect())
    }

    async fn validate_values(
        &self,
        values: &BTreeMap<String, Value>,
    ) -> Result<Vec<String>, String> {
        let definitions = self.store.list_definitions().await?;
        let mut errors = Vec::new();
        for definition in &definitions {
            let value = values
                .get(definition.key.as_str())
                .unwrap_or(&definition.default_value);
            validate_definition(definition, value, &mut errors);
        }
        let single = values
            .get("task_runner.ai.tool_result_max_chars")
            .and_then(Value::as_i64);
        let total = values
            .get("task_runner.ai.tool_results_total_max_chars")
            .and_then(Value::as_i64);
        if let (Some(single), Some(total)) = (single, total) {
            if total < single {
                errors.push(
                    "task_runner.ai.tool_results_total_max_chars must be greater than or equal to task_runner.ai.tool_result_max_chars"
                        .to_string(),
                );
            }
        }
        Ok(errors)
    }

    async fn publish_consul(
        &self,
        environment: &str,
        revision: i64,
        definitions: &[ConfigDefinitionRecord],
        values: &BTreeMap<String, Value>,
    ) -> Result<(), String> {
        let Some(consul) = self.config.consul_http_addr.as_deref() else {
            return Ok(());
        };
        let shared = compatibility_env(definitions, values, |definition| {
            definition.scope == "shared"
        });
        self.put_consul(
            consul,
            format!("chatos/{environment}/shared/config").as_str(),
            &json!({ "revision": revision, "env": shared }),
        )
        .await?;
        let services = known_services(definitions);
        for service_name in services {
            let env = compatibility_env(definitions, values, |definition| {
                definition.service_name.as_deref() == Some(service_name.as_str())
            });
            self.put_consul(
                consul,
                format!("chatos/{environment}/services/{service_name}/config").as_str(),
                &json!({ "revision": revision, "env": env }),
            )
            .await?;
        }
        Ok(())
    }

    async fn put_consul(&self, base_url: &str, key: &str, value: &Value) -> Result<(), String> {
        let response = self
            .http
            .put(format!("{}/v1/kv/{key}", base_url.trim_end_matches('/')))
            .body(serde_json::to_vec(value).map_err(|err| err.to_string())?)
            .send()
            .await
            .map_err(|err| format!("Consul write {key} failed: {err}"))?;
        if !response.status().is_success() {
            return Err(format!("Consul write {key} returned {}", response.status()));
        }
        Ok(())
    }

    async fn audit(
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

    async fn purge_user_preferences_from_config_center(&self) -> Result<(), String> {
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

    async fn migrate_agent_max_iterations_config(&self) -> Result<(), String> {
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

fn migrate_agent_iteration_values(
    values: &mut BTreeMap<String, Value>,
    insert_default: bool,
) -> bool {
    migrate_agent_iteration_values_with_fallback(
        values,
        json!(chatos_agent::DEFAULT_AGENT_MAX_ITERATIONS),
        insert_default,
    )
}

fn migrate_agent_iteration_values_with_fallback(
    values: &mut BTreeMap<String, Value>,
    fallback: Value,
    insert_default: bool,
) -> bool {
    let current = values
        .get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
        .cloned();
    let legacy = LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS
        .iter()
        .find_map(|key| values.get(*key).cloned());
    let selected = current.or(legacy).or(insert_default.then_some(fallback));
    let mut changed = false;
    for key in LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS {
        changed |= values.remove(*key).is_some();
    }
    if let Some(selected) = selected {
        if values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY) != Some(&selected) {
            values.insert(
                chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
                selected,
            );
            changed = true;
        }
    }
    changed
}

fn migrate_agent_iteration_changed_keys(keys: &mut Vec<String>) -> bool {
    let had_legacy = keys
        .iter()
        .any(|key| LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str()));
    if !had_legacy {
        return false;
    }
    keys.retain(|key| !LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str()));
    if !keys
        .iter()
        .any(|key| key == chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
    {
        keys.push(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string());
    }
    keys.sort();
    true
}

fn system_user() -> CurrentUser {
    CurrentUser {
        user_id: "system".to_string(),
        username: "system".to_string(),
        display_name: "System".to_string(),
        role: "super_admin".to_string(),
    }
}

fn validate_definition(
    definition: &ConfigDefinitionRecord,
    value: &Value,
    errors: &mut Vec<String>,
) {
    if value.is_null() {
        if !definition.nullable {
            errors.push(format!("{} cannot be null", definition.key));
        }
        return;
    }
    match definition.value_type.as_str() {
        "integer" | "duration_ms" | "bytes" => {
            let Some(number) = value.as_i64() else {
                errors.push(format!("{} must be an integer", definition.key));
                return;
            };
            if definition.min.is_some_and(|min| number < min) {
                errors.push(format!(
                    "{} must be greater than or equal to {}",
                    definition.key,
                    definition.min.unwrap_or_default()
                ));
            }
            if definition.max.is_some_and(|max| number > max) {
                errors.push(format!(
                    "{} must be less than or equal to {}",
                    definition.key,
                    definition.max.unwrap_or_default()
                ));
            }
        }
        "boolean" => {
            if !value.is_boolean() {
                errors.push(format!("{} must be a boolean", definition.key));
            }
        }
        "enum" => {
            let Some(text) = value.as_str() else {
                errors.push(format!("{} must be a string", definition.key));
                return;
            };
            if !definition.enum_options.iter().any(|option| option == text) {
                errors.push(format!(
                    "{} must be one of {}",
                    definition.key,
                    definition.enum_options.join(", ")
                ));
            }
        }
        "string" | "secret_ref" if !value.is_string() => {
            errors.push(format!("{} must be a string", definition.key));
        }
        _ => {}
    }
}

fn build_snapshot(
    environment: &str,
    service_name: &str,
    revision: i64,
    definitions: &[ConfigDefinitionRecord],
    all_values: &BTreeMap<String, Value>,
) -> Result<ConfigSnapshot, String> {
    let values = definitions
        .iter()
        .filter(|definition| {
            definition.scope == "shared" || definition.service_name.as_deref() == Some(service_name)
        })
        .map(|definition| {
            (
                definition.key.clone(),
                all_values
                    .get(definition.key.as_str())
                    .cloned()
                    .unwrap_or_else(|| definition.default_value.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let env = compatibility_env(definitions, &values, |definition| {
        definition.scope == "shared" || definition.service_name.as_deref() == Some(service_name)
    });
    let checksum = checksum(&json!({ "values": values, "env": env }))?;
    Ok(ConfigSnapshot {
        environment: environment.to_string(),
        service_name: service_name.to_string(),
        revision,
        checksum,
        values,
        env,
        generated_at: Utc::now().to_rfc3339(),
        stale: false,
        source: Some("configuration_center".to_string()),
    })
}

fn compatibility_env<F>(
    definitions: &[ConfigDefinitionRecord],
    values: &BTreeMap<String, Value>,
    include: F,
) -> BTreeMap<String, String>
where
    F: Fn(&ConfigDefinitionRecord) -> bool,
{
    let mut env = BTreeMap::new();
    for definition in definitions.iter().filter(|definition| include(definition)) {
        let Some(value) = values.get(definition.key.as_str()) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        let text = match value {
            Value::String(value) => value.clone(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
            Value::Null => continue,
        };
        for alias in &definition.env_aliases {
            env.insert(alias.clone(), text.clone());
        }
    }
    env
}

fn checksum(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|err| err.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn changed_keys(
    current: &BTreeMap<String, Value>,
    target: &BTreeMap<String, Value>,
) -> Vec<String> {
    current
        .keys()
        .chain(target.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|key| current.get(key) != target.get(key))
        .collect()
}

fn known_services(definitions: &[ConfigDefinitionRecord]) -> BTreeSet<String> {
    let mut services = [
        "chatos-backend",
        "task-runner",
        "user-service",
        "project-service",
        "plugin-management-service",
        "local-connector-service",
        "sandbox-manager",
        "memory-engine",
        "official-website",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect::<BTreeSet<_>>();
    services.extend(
        definitions
            .iter()
            .filter_map(|definition| definition.service_name.clone()),
    );
    services
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_agent_iteration_values_collapse_to_one_key() {
        let mut values = BTreeMap::from([
            ("chatos.ai.max_iterations".to_string(), json!(700)),
            (
                "task_runner.execution.max_iterations".to_string(),
                json!(300),
            ),
        ]);

        assert!(migrate_agent_iteration_values(&mut values, true));
        assert_eq!(
            values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY),
            Some(&json!(700))
        );
        assert!(!values.contains_key("chatos.ai.max_iterations"));
        assert!(!values.contains_key("task_runner.execution.max_iterations"));
    }

    #[test]
    fn explicit_shared_agent_value_wins_over_legacy_values() {
        let mut values = BTreeMap::from([
            (
                chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
                json!(900),
            ),
            ("chatos.ai.max_iterations".to_string(), json!(700)),
        ]);

        assert!(migrate_agent_iteration_values(&mut values, true));
        assert_eq!(
            values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY),
            Some(&json!(900))
        );
    }

    #[test]
    fn empty_draft_does_not_gain_an_unrequested_change() {
        let mut values = BTreeMap::new();
        assert!(!migrate_agent_iteration_values(&mut values, false));
        assert!(values.is_empty());
    }

    #[test]
    fn audit_keys_replace_legacy_agent_keys_once() {
        let mut keys = vec![
            "chatos.ai.max_iterations".to_string(),
            "task_runner.execution.max_iterations".to_string(),
            "shared.logging.level".to_string(),
        ];

        assert!(migrate_agent_iteration_changed_keys(&mut keys));
        assert_eq!(
            keys.iter()
                .filter(|key| *key == chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
                .count(),
            1
        );
        assert!(!keys
            .iter()
            .any(|key| LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str())));
    }
}
