// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::memory_context::{MemoryEngineRecordWriter, MemoryRecordScope};

use super::TaskRuntimeBuilder;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemoryRuntimeConfig {
    pub base_url: String,
    pub source_id: String,
    #[serde(default, skip_serializing)]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing)]
    pub internal_secret: Option<String>,
    #[serde(default, skip_serializing)]
    pub internal_caller: Option<String>,
    #[serde(default, skip_serializing)]
    pub runtime_assertion_secret: Option<String>,
    #[serde(default, skip_serializing)]
    pub runtime_assertion_key_id: Option<String>,
    #[serde(default, skip_serializing)]
    pub runtime_assertion_issuer: Option<String>,
    #[serde(default, skip_serializing)]
    pub runtime_identity_revision: Option<String>,
    #[serde(default = "default_memory_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_memory_compose_context")]
    pub compose_context: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_scope: Option<MemoryRecordScope>,
}

impl TaskMemoryRuntimeConfig {
    pub fn new(base_url: impl Into<String>, source_id: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            source_id: source_id.into(),
            access_token: None,
            internal_secret: None,
            internal_caller: None,
            runtime_assertion_secret: None,
            runtime_assertion_key_id: None,
            runtime_assertion_issuer: None,
            runtime_identity_revision: None,
            timeout_ms: default_memory_timeout_ms(),
            compose_context: default_memory_compose_context(),
            record_scope: None,
        }
    }

    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_compose_context(mut self, compose_context: bool) -> Self {
        self.compose_context = compose_context;
        self
    }

    pub fn with_record_scope(mut self, record_scope: Option<MemoryRecordScope>) -> Self {
        self.record_scope = record_scope;
        self
    }

    pub fn with_access_token(mut self, access_token: Option<String>) -> Self {
        self.access_token = normalize_optional_token(access_token);
        self
    }

    pub fn with_internal_service_auth(
        mut self,
        caller: impl Into<String>,
        secret: Option<String>,
    ) -> Self {
        self.internal_caller = normalize_optional_token(Some(caller.into()));
        self.internal_secret = normalize_optional_token(secret);
        self
    }

    pub fn with_runtime_context_assertion_issuer(
        mut self,
        secret: impl Into<String>,
        key_id: impl Into<String>,
        issuer: impl Into<String>,
        runtime_identity_revision: impl Into<String>,
    ) -> Self {
        self.runtime_assertion_secret = normalize_optional_token(Some(secret.into()));
        self.runtime_assertion_key_id = normalize_optional_token(Some(key_id.into()));
        self.runtime_assertion_issuer = normalize_optional_token(Some(issuer.into()));
        self.runtime_identity_revision =
            normalize_optional_token(Some(runtime_identity_revision.into()));
        self
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    pub fn apply_to_builder(
        &self,
        builder: TaskRuntimeBuilder,
    ) -> Result<TaskRuntimeBuilder, String> {
        let client = self.build_client()?;
        self.apply_client_to_builder(builder, client)
    }

    pub fn apply_to_builder_with_http_client(
        &self,
        builder: TaskRuntimeBuilder,
        http_client: reqwest::Client,
    ) -> Result<TaskRuntimeBuilder, String> {
        let client = self.build_client_with_http_client(http_client)?;
        self.apply_client_to_builder(builder, client)
    }

    fn apply_client_to_builder(
        &self,
        mut builder: TaskRuntimeBuilder,
        client: memory_engine_sdk::MemoryEngineClient,
    ) -> Result<TaskRuntimeBuilder, String> {
        let assertion_issuer = self.runtime_context_assertion_issuer()?;
        if self.compose_context {
            builder = builder.with_memory_composer(
                crate::memory_context::MemoryContextComposer::from_client(client.clone())
                    .with_runtime_context_assertion_issuer(assertion_issuer.clone()),
            );
        }
        if let Some(record_scope) = self.record_scope.clone() {
            let writer = MemoryEngineRecordWriter::from_client(client.clone(), record_scope)
                .with_runtime_context_assertion_issuer(assertion_issuer);
            // Task conversation records are authoritative Memory Engine data.
            // Do not hide persistence failures and let the task continue with a
            // history that the next model turn cannot recover.
            builder = builder.with_record_writer(writer);
        }
        Ok(builder)
    }

    fn runtime_context_assertion_issuer(
        &self,
    ) -> Result<memory_engine_sdk::RuntimeContextAssertionIssuer, String> {
        memory_engine_sdk::RuntimeContextAssertionIssuer::new(
            required_runtime_assertion_value(
                "runtime_assertion_secret",
                self.runtime_assertion_secret.as_deref(),
            )?,
            required_runtime_assertion_value(
                "runtime_assertion_key_id",
                self.runtime_assertion_key_id.as_deref(),
            )?,
            required_runtime_assertion_value(
                "runtime_assertion_issuer",
                self.runtime_assertion_issuer.as_deref(),
            )?,
            required_runtime_assertion_value(
                "runtime_identity_revision",
                self.runtime_identity_revision.as_deref(),
            )?,
            60,
        )
    }

    fn build_client(&self) -> Result<memory_engine_sdk::MemoryEngineClient, String> {
        let http_client = reqwest::Client::builder()
            .timeout(self.timeout())
            .build()
            .map_err(|err| err.to_string())?;
        self.build_client_with_http_client(http_client)
    }

    fn build_client_with_http_client(
        &self,
        http_client: reqwest::Client,
    ) -> Result<memory_engine_sdk::MemoryEngineClient, String> {
        let mut client = memory_engine_sdk::MemoryEngineClient::new_direct_with_http_client(
            self.base_url.clone(),
            self.source_id.clone(),
            http_client,
        );
        if let Some(access_token) = self.access_token.as_deref() {
            client = client.with_bearer_token(access_token);
        } else if let Some(internal_secret) = self.internal_secret.as_deref() {
            let caller = self.internal_caller.as_deref().ok_or_else(|| {
                "Memory Engine internal caller is required when a signing secret is configured"
                    .to_string()
            })?;
            client = client.with_internal_service_auth(caller, internal_secret);
        }
        Ok(client)
    }
}

fn default_memory_timeout_ms() -> u64 {
    30_000
}

fn default_memory_compose_context() -> bool {
    true
}

fn normalize_optional_token(token: Option<String>) -> Option<String> {
    token.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn required_runtime_assertion_value<'a>(
    field: &str,
    value: Option<&'a str>,
) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Memory Engine {field} is required"))
}
