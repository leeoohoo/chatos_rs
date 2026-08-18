// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppState {
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
        let mut snapshot = self
            .store
            .get_active_snapshot(environment, service_name)
            .await?
            .ok_or_else(|| format!("No published snapshot for {environment}/{service_name}"))?;
        if let Some(pressure) = self.store.get_pressure_state(environment).await? {
            overlay_pressure_state(&mut snapshot, &pressure)?;
        }
        Ok(snapshot)
    }

    pub(super) async fn publish_values(
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

    pub(super) async fn default_values(&self) -> Result<BTreeMap<String, Value>, String> {
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
            .get(TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY)
            .and_then(Value::as_i64);
        let total = values
            .get(TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY)
            .and_then(Value::as_i64);
        if let (Some(single), Some(total)) = (single, total) {
            if total < single {
                errors.push(
                    "task_runner.ai.tool_results_total_max_chars must be greater than or equal to task_runner.ai.tool_result_max_chars"
                        .to_string(),
                );
            }
        }
        let elevated = values
            .get(MEMORY_ENGINE_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY)
            .and_then(Value::as_i64);
        let critical = values
            .get(MEMORY_ENGINE_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY)
            .and_then(Value::as_i64);
        if let (Some(elevated), Some(critical)) = (elevated, critical) {
            if critical <= elevated {
                errors.push(
                    "memory_engine.pressure.queue_critical_messages must be greater than memory_engine.pressure.queue_elevated_messages"
                        .to_string(),
                );
            }
        }
        let controller_interval_ms = values
            .get(PLATFORM_PRESSURE_CONTROLLER_INTERVAL_MS_CONFIG_KEY)
            .and_then(Value::as_i64);
        let signal_ttl_seconds = values
            .get(PLATFORM_PRESSURE_SIGNAL_TTL_SECONDS_CONFIG_KEY)
            .and_then(Value::as_i64);
        if let (Some(interval_ms), Some(ttl_seconds)) = (controller_interval_ms, signal_ttl_seconds)
        {
            if ttl_seconds.saturating_mul(1_000) <= interval_ms {
                errors.push(
                    "platform.pressure.controller.signal_ttl_seconds must exceed platform.pressure.controller.interval_ms"
                        .to_string(),
                );
            }
        }
        for key in [
            SHARED_MCP_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY,
            CONFIGURATION_CENTER_MCP_MANAGEMENT_BASE_URL_CONFIG_KEY,
        ] {
            let is_https = values
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.trim().starts_with("https://"));
            if !is_https {
                errors.push(format!(
                    "{key} must use https:// because MCP Management internal APIs require mTLS"
                ));
            }
        }
        for key in [
            CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
            TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
            MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL_CONFIG_KEY,
        ] {
            let is_https = values
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.trim().starts_with("https://"));
            if !is_https {
                errors.push(format!(
                    "{key} must use https:// because Project Service internal APIs require mTLS"
                ));
            }
        }
        let user_service_internal_is_https = values
            .get(PROJECT_SERVICE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY)
            .and_then(Value::as_str)
            .is_some_and(|value| value.trim().starts_with("https://"));
        if !user_service_internal_is_https {
            errors.push(format!(
                "{} must use https:// because User Service internal APIs require mTLS",
                PROJECT_SERVICE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY
            ));
        }
        for key in [
            MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY,
            SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY,
        ] {
            let is_https = values
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.trim().starts_with("https://"));
            if !is_https {
                errors.push(format!(
                    "{key} must use https:// because Plugin Management internal APIs require mTLS"
                ));
            }
        }
        validate_sandbox_manager_mtls_invariants(values, &mut errors);
        for key in [
            CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            TASK_RUNNER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            USER_SERVICE_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
        ] {
            let is_https = values
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.trim().starts_with("https://"));
            if !is_https {
                errors.push(format!(
                    "{key} must use https:// because Memory Engine internal APIs require mTLS"
                ));
            }
        }
        for key in [
            CHATOS_TASK_RUNNER_INTERNAL_BASE_URL_CONFIG_KEY,
            MCP_MANAGEMENT_TASK_RUNNER_SERVICE_BASE_URL_CONFIG_KEY,
            PROJECT_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            USER_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY,
        ] {
            let is_https = values
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.trim().starts_with("https://"));
            if !is_https {
                errors.push(format!(
                    "{key} must use https:// because Task Runner internal APIs require mTLS"
                ));
            }
        }
        validate_chatos_mtls_invariants(values, &mut errors);
        let public_port = values
            .get(MCP_MANAGEMENT_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        let internal_mtls_port = values
            .get(MCP_MANAGEMENT_INTERNAL_MTLS_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        if public_port.is_some() && public_port == internal_mtls_port {
            errors.push(
                "mcp_management.runtime.internal_mtls_port must differ from mcp_management.runtime.port"
                    .to_string(),
            );
        }
        let task_runner_public_port = values
            .get(TASK_RUNNER_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        let task_runner_internal_mtls_port = values
            .get(TASK_RUNNER_INTERNAL_MTLS_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        if task_runner_public_port.is_some()
            && task_runner_public_port == task_runner_internal_mtls_port
        {
            errors.push(
                "task_runner.runtime.internal_mtls_port must differ from task_runner.runtime.port"
                    .to_string(),
            );
        }
        let memory_engine_public_port = values
            .get(MEMORY_ENGINE_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        let memory_engine_internal_mtls_port = values
            .get(MEMORY_ENGINE_INTERNAL_MTLS_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        if memory_engine_public_port.is_some()
            && memory_engine_public_port == memory_engine_internal_mtls_port
        {
            errors.push(
                "memory_engine.runtime.internal_mtls_port must differ from memory_engine.runtime.port"
                    .to_string(),
            );
        }
        let project_service_public_port = values
            .get(PROJECT_SERVICE_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        let project_service_internal_mtls_port = values
            .get(PROJECT_SERVICE_INTERNAL_MTLS_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        if project_service_public_port.is_some()
            && project_service_public_port == project_service_internal_mtls_port
        {
            errors.push(
                "project_service.runtime.internal_mtls_port must differ from project_service.runtime.port"
                .to_string(),
            );
        }
        let user_service_public_port = values
            .get(USER_SERVICE_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        let user_service_internal_mtls_port = values
            .get(USER_SERVICE_INTERNAL_MTLS_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        if user_service_public_port.is_some()
            && user_service_public_port == user_service_internal_mtls_port
        {
            errors.push(
                "user_service.runtime.internal_mtls_port must differ from user_service.runtime.port"
                    .to_string(),
            );
        }
        let plugin_management_public_port = values
            .get(PLUGIN_MANAGEMENT_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        let plugin_management_internal_mtls_port = values
            .get(PLUGIN_MANAGEMENT_INTERNAL_MTLS_PORT_CONFIG_KEY)
            .and_then(Value::as_i64);
        if plugin_management_public_port.is_some()
            && plugin_management_public_port == plugin_management_internal_mtls_port
        {
            errors.push(
                "plugin_management.runtime.internal_mtls_port must differ from plugin_management.runtime.port"
                    .to_string(),
            );
        }
        Ok(errors)
    }
}

pub(super) fn validate_chatos_mtls_invariants(
    values: &BTreeMap<String, Value>,
    errors: &mut Vec<String>,
) {
    for key in [
        TASK_RUNNER_CHATOS_CALLBACK_URL_CONFIG_KEY,
        MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL_CONFIG_KEY,
    ] {
        let is_https = values
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| value.trim().starts_with("https://"));
        if !is_https {
            errors.push(format!(
                "{key} must use https:// because ChatOS internal APIs require mTLS"
            ));
        }
    }

    let public_port = values
        .get(CHATOS_BACKEND_PORT_CONFIG_KEY)
        .and_then(Value::as_i64);
    let internal_mtls_port = values
        .get(CHATOS_INTERNAL_MTLS_PORT_CONFIG_KEY)
        .and_then(Value::as_i64);
    if public_port.is_some() && public_port == internal_mtls_port {
        errors.push(
            "chatos.runtime.internal_mtls_port must differ from chatos.runtime.port".to_string(),
        );
    }
}

pub(super) fn validate_sandbox_manager_mtls_invariants(
    values: &BTreeMap<String, Value>,
    errors: &mut Vec<String>,
) {
    for key in [
        PROJECT_SERVICE_SANDBOX_MANAGER_BASE_URL_CONFIG_KEY,
        MCP_MANAGEMENT_SANDBOX_MANAGER_SERVICE_BASE_URL_CONFIG_KEY,
    ] {
        let is_https = values
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| value.trim().starts_with("https://"));
        if !is_https {
            errors.push(format!(
                "{key} must use https:// because Sandbox Manager internal APIs require mTLS"
            ));
        }
    }

    let public_port = values
        .get(SANDBOX_MANAGER_PORT_CONFIG_KEY)
        .and_then(Value::as_i64);
    let internal_mtls_port = values
        .get(SANDBOX_MANAGER_INTERNAL_MTLS_PORT_CONFIG_KEY)
        .and_then(Value::as_i64);
    if public_port.is_some() && public_port == internal_mtls_port {
        errors.push(
            "sandbox_manager.runtime.internal_mtls_port must differ from sandbox_manager.runtime.port"
                .to_string(),
        );
    }
}

pub(super) fn overlay_pressure_state(
    snapshot: &mut ConfigSnapshot,
    pressure: &PlatformPressureStateRecord,
) -> Result<(), String> {
    snapshot.values.insert(
        PLATFORM_PRESSURE_LEVEL_CONFIG_KEY.to_string(),
        json!(pressure.level.as_str()),
    );
    snapshot.checksum = checksum(&json!({
        "values": snapshot.values,
        "env": snapshot.env,
    }))?;
    Ok(())
}
