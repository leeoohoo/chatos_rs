// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
        let http = build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
            .map_err(|err| format!("build configuration center HTTP client failed: {err}"))?;
        let state = Self {
            http,
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

    pub(crate) fn http_client(&self) -> &reqwest::Client {
        &self.http
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
}
