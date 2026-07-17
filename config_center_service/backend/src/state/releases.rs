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
        self.store
            .get_active_snapshot(environment, service_name)
            .await?
            .ok_or_else(|| format!("No published snapshot for {environment}/{service_name}"))
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
}
