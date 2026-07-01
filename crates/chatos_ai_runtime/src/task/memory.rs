// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::memory_context::{
    BestEffortMemoryRecordWriter, MemoryEngineRecordWriter, MemoryRecordScope,
};
use crate::runtime::MemoryContextOverflowRecovery;

use super::TaskRuntimeBuilder;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemoryRuntimeConfig {
    pub base_url: String,
    pub source_id: String,
    #[serde(default, skip_serializing)]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing)]
    pub operator_token: Option<String>,
    #[serde(default = "default_memory_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_memory_compose_context")]
    pub compose_context: bool,
    #[serde(default = "default_retry_on_context_overflow")]
    pub retry_on_context_overflow: bool,
    #[serde(default = "default_active_summary_poll_interval_ms")]
    pub active_summary_poll_interval_ms: u64,
    #[serde(default = "default_active_summary_poll_timeout_ms")]
    pub active_summary_poll_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_scope: Option<MemoryRecordScope>,
}

impl TaskMemoryRuntimeConfig {
    pub fn new(base_url: impl Into<String>, source_id: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            source_id: source_id.into(),
            access_token: None,
            operator_token: None,
            timeout_ms: default_memory_timeout_ms(),
            compose_context: default_memory_compose_context(),
            retry_on_context_overflow: default_retry_on_context_overflow(),
            active_summary_poll_interval_ms: default_active_summary_poll_interval_ms(),
            active_summary_poll_timeout_ms: default_active_summary_poll_timeout_ms(),
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

    pub fn with_operator_token(mut self, operator_token: Option<String>) -> Self {
        self.operator_token = normalize_optional_token(operator_token);
        self
    }

    pub fn with_retry_on_context_overflow(mut self, retry_on_context_overflow: bool) -> Self {
        self.retry_on_context_overflow = retry_on_context_overflow;
        self
    }

    pub fn with_active_summary_poll_interval_ms(
        mut self,
        active_summary_poll_interval_ms: u64,
    ) -> Self {
        self.active_summary_poll_interval_ms = active_summary_poll_interval_ms;
        self
    }

    pub fn with_active_summary_poll_timeout_ms(
        mut self,
        active_summary_poll_timeout_ms: u64,
    ) -> Self {
        self.active_summary_poll_timeout_ms = active_summary_poll_timeout_ms;
        self
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    pub fn apply_to_builder(
        &self,
        mut builder: TaskRuntimeBuilder,
    ) -> Result<TaskRuntimeBuilder, String> {
        let client = self.build_client()?;
        if self.compose_context {
            builder = builder.with_memory_composer(
                crate::memory_context::MemoryContextComposer::from_client(client.clone()),
            );
        }
        if let Some(record_scope) = self.record_scope.clone() {
            let writer = MemoryEngineRecordWriter::from_client(client.clone(), record_scope);
            builder = builder.with_record_writer(BestEffortMemoryRecordWriter::new(writer));
        }
        if self.retry_on_context_overflow {
            builder = builder.with_context_overflow_recovery(Some(
                MemoryContextOverflowRecovery::new()
                    .with_trigger_reason("context_overflow")
                    .with_poll_interval(Duration::from_millis(
                        self.active_summary_poll_interval_ms.max(1_000),
                    ))
                    .with_poll_timeout(Duration::from_millis(
                        self.active_summary_poll_timeout_ms.max(10_000),
                    )),
            ));
        }
        Ok(builder)
    }

    fn build_client(&self) -> Result<memory_engine_sdk::MemoryEngineClient, String> {
        let mut client = memory_engine_sdk::MemoryEngineClient::new_direct(
            self.base_url.clone(),
            self.timeout(),
            self.source_id.clone(),
        )?;
        if let Some(access_token) = self.access_token.as_deref() {
            client = client.with_bearer_token(access_token);
        } else if let Some(operator_token) = self.operator_token.as_deref() {
            client = client.with_operator_token(operator_token);
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

fn default_retry_on_context_overflow() -> bool {
    true
}

fn default_active_summary_poll_interval_ms() -> u64 {
    10_000
}

fn default_active_summary_poll_timeout_ms() -> u64 {
    120_000
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
