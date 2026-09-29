// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{
    AiRuntime, ContextualTurnRunner, MemoryContextComposer, MemoryEngineRecordWriter,
    MemoryRecordScope,
};
use memory_engine_sdk::MemoryEngineClient;
use std::time::Duration;

/// Process-local retained Memory adapter. Authentication values are accepted
/// only as transient strings and this type deliberately implements neither
/// serialization nor debug formatting.
pub struct LocalMemoryRuntimeConfig {
    base_url: String,
    source_id: String,
    timeout: Duration,
    access_token: Option<String>,
    internal_caller: Option<String>,
    internal_secret: Option<String>,
}

impl LocalMemoryRuntimeConfig {
    pub fn new(
        base_url: impl Into<String>,
        source_id: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            source_id: source_id.into(),
            timeout,
            access_token: None,
            internal_caller: None,
            internal_secret: None,
        }
    }

    pub fn with_access_token(mut self, access_token: Option<String>) -> Self {
        self.access_token = normalized(access_token);
        self
    }

    pub fn with_internal_service_auth(
        mut self,
        caller: Option<String>,
        secret: Option<String>,
    ) -> Self {
        self.internal_caller = normalized(caller);
        self.internal_secret = normalized(secret);
        self
    }

    pub fn source_id(&self) -> &str {
        self.source_id.trim()
    }

    pub fn build_runner(&self) -> Result<ContextualTurnRunner, String> {
        let base_url = required("Memory base_url", &self.base_url)?;
        let source_id = required("Memory source_id", &self.source_id)?;
        if self.timeout.is_zero() {
            return Err("Memory timeout must be greater than zero".to_string());
        }
        let http = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| format!("build Memory HTTP client failed: {error}"))?;
        let mut client = MemoryEngineClient::new_direct_with_http_client(base_url, source_id, http);
        if let Some(access_token) = self.access_token.as_deref() {
            client = client.with_bearer_token(access_token);
        } else if let Some(internal_secret) = self.internal_secret.as_deref() {
            let caller = self.internal_caller.as_deref().ok_or_else(|| {
                "Memory internal caller is required when an internal secret is configured"
                    .to_string()
            })?;
            client = client.with_internal_service_auth(caller, internal_secret);
        }
        let composer = MemoryContextComposer::from_client(client.clone());
        let writer = MemoryEngineRecordWriter::from_client(
            client,
            MemoryRecordScope::per_record_tenant("tenant_id"),
        );
        Ok(AiRuntime::builder()
            .with_memory_composer(composer)
            .with_record_writer(writer)
            .build_contextual_turn_runner())
    }
}

fn required(label: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} must not be empty"))
    } else {
        Ok(value.to_string())
    }
}

fn normalized(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_non_secret_memory_configuration() {
        let config = LocalMemoryRuntimeConfig::new(
            "http://127.0.0.1:8080",
            "local_agent",
            Duration::from_secs(1),
        );
        assert_eq!(config.source_id(), "local_agent");
        config.build_runner().expect("runner");

        let invalid = LocalMemoryRuntimeConfig::new(" ", "local_agent", Duration::from_secs(1));
        assert!(invalid.build_runner().is_err());
    }

    #[test]
    fn internal_secret_requires_a_caller() {
        let config = LocalMemoryRuntimeConfig::new(
            "http://127.0.0.1:8080",
            "local_agent",
            Duration::from_secs(1),
        )
        .with_internal_service_auth(None, Some("secret".to_string()));
        assert!(config.build_runner().is_err());
    }
}
