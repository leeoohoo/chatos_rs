// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let postgres_config = chatos_postgres::PostgresConfig::from_env(
            config.database_url.clone(),
            "configuration-center",
            "CONFIG_CENTER",
        )
        .map_err(|err| err.to_string())?;
        let pool = chatos_postgres::connect(&postgres_config)
            .await
            .map_err(|err| format!("connect configuration center PostgreSQL failed: {err}"))?;
        let store = AppStore::new(pool);
        store.initialize().await?;
        store
            .delete_definitions(USER_PREFERENCE_CONFIG_KEYS)
            .await?;
        store
            .delete_definitions(LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS)
            .await?;
        store.delete_definitions(RETIRED_CONFIG_KEYS).await?;
        for definition in builtin_definitions() {
            store.upsert_definition(&definition).await?;
        }
        let http = build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
            .map_err(|err| format!("build configuration center HTTP client failed: {err}"))?;
        let mcp_management_http = build_mcp_management_mtls_client(&config)?;
        let memory_engine_http = build_memory_engine_mtls_client(&config)?;
        let mut state = Self {
            http,
            mcp_management_http,
            memory_engine_http,
            config,
            store,
        };
        state
            .ensure_initial_release(state.config.default_environment.as_str())
            .await?;
        state.purge_user_preferences_from_config_center().await?;
        state.purge_retired_config_keys().await?;
        state.migrate_agent_max_iterations_config().await?;
        state.migrate_task_runner_runtime_config().await?;
        state.migrate_mcp_management_runtime_config().await?;
        state.migrate_local_connector_runtime_config().await?;
        state.migrate_memory_engine_runtime_config().await?;
        state.migrate_platform_pressure_config().await?;
        state.migrate_internal_request_security_config().await?;
        state.migrate_plugin_management_runtime_config().await?;
        state.migrate_user_service_runtime_config().await?;
        state.migrate_user_service_smtp_config().await?;
        state.migrate_chatos_ui_config().await?;
        state.migrate_postgres_pool_config().await?;
        state.activate_managed_postgres_pool().await?;
        Ok(state)
    }

    async fn activate_managed_postgres_pool(&mut self) -> Result<(), String> {
        let effective = self
            .effective(self.config.default_environment.as_str())
            .await?;
        let values = &effective.values;
        let required_u32 = |key: &str| {
            values
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| format!("{key} must be a valid unsigned integer"))
        };
        let required_duration = |key: &str| {
            values
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .map(std::time::Duration::from_millis)
                .ok_or_else(|| format!("{key} must be a valid duration in milliseconds"))
        };
        let mut config = chatos_postgres::PostgresConfig::new(self.config.database_url.clone())
            .and_then(|config| config.with_application_name("configuration-center"))
            .map_err(|error| error.to_string())?;
        config.max_connections =
            required_u32("configuration_center.postgres.pool.max_connections")?;
        config.min_connections =
            required_u32("configuration_center.postgres.pool.min_connections")?;
        config.acquire_timeout =
            required_duration("configuration_center.postgres.pool.acquire_timeout_ms")?;
        config.idle_timeout =
            required_duration("configuration_center.postgres.pool.idle_timeout_ms")?;
        config.max_lifetime =
            required_duration("configuration_center.postgres.pool.max_lifetime_ms")?;
        config.statement_timeout =
            required_duration("configuration_center.postgres.statement_timeout_ms")?;
        config.lock_timeout = required_duration("configuration_center.postgres.lock_timeout_ms")?;
        config.validate().map_err(|error| error.to_string())?;

        let pool = chatos_postgres::connect(&config).await.map_err(|error| {
            format!("activate managed Configuration Center pool failed: {error}")
        })?;
        let store = AppStore::new(pool);
        store.initialize().await?;
        self.store = store;
        tracing::info!(
            max_connections = config.max_connections,
            min_connections = config.min_connections,
            "activated managed Configuration Center PostgreSQL pool"
        );
        Ok(())
    }

    pub(crate) fn http_client(&self) -> &reqwest::Client {
        &self.http
    }

    pub(crate) fn mcp_management_http_client(&self) -> &reqwest::Client {
        &self.mcp_management_http
    }

    pub(crate) fn memory_engine_http_client(&self) -> &reqwest::Client {
        &self.memory_engine_http
    }

    pub async fn ensure_initial_release(&self, environment: &str) -> Result<(), String> {
        if self.store.get_active(environment).await?.is_some() {
            return Ok(());
        }
        let mut values = self.default_values().await?;
        let bootstrap_override = explicit_non_production_user_admin_bootstrap_override()?;
        apply_user_admin_bootstrap_override(&mut values, bootstrap_override.as_ref());
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

fn build_mcp_management_mtls_client(config: &AppConfig) -> Result<reqwest::Client, String> {
    let ca_pem =
        std::fs::read(config.mcp_management_mtls_ca_cert_path.as_path()).map_err(|err| {
            format!(
                "read MCP Management mTLS CA certificate {} failed: {err}",
                config.mcp_management_mtls_ca_cert_path.display()
            )
        })?;
    let identity_pem = std::fs::read(config.mcp_management_mtls_client_identity_path.as_path())
        .map_err(|err| {
            format!(
                "read MCP Management mTLS client identity {} failed: {err}",
                config.mcp_management_mtls_client_identity_path.display()
            )
        })?;
    let ca = reqwest::Certificate::from_pem(ca_pem.as_slice())
        .map_err(|err| format!("parse MCP Management mTLS CA certificate failed: {err}"))?;
    let identity = reqwest::Identity::from_pem(identity_pem.as_slice())
        .map_err(|err| format!("parse MCP Management mTLS client identity failed: {err}"))?;
    reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .timeout(config.user_service_request_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .add_root_certificate(ca)
        .identity(identity)
        .build()
        .map_err(|err| format!("build MCP Management mTLS client failed: {err}"))
}

fn build_memory_engine_mtls_client(config: &AppConfig) -> Result<reqwest::Client, String> {
    chatos_service_runtime::build_mtls_http_client(
        HttpClientTimeouts::new(config.user_service_request_timeout),
        config.memory_engine_mtls_ca_cert_path.as_path(),
        config.memory_engine_mtls_client_identity_path.as_path(),
    )
    .map_err(|err| format!("build Memory Engine mTLS client failed: {err}"))
}
