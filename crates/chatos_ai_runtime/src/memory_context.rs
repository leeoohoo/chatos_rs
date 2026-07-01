// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use memory_engine_sdk::{
    ComposeContextPolicy, ComposeContextResponse, MemoryEngineClient,
    RunThreadActiveSummaryResponse, SdkComposeContextRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{sleep, Instant};

use crate::tool_runtime::ToolResultModelBudgetLimits;

#[path = "memory_context/compose_items.rs"]
mod compose_items;
#[path = "memory_context/log_summary.rs"]
mod log_summary;
#[path = "memory_context/record_writer.rs"]
mod record_writer;

pub use compose_items::{
    compose_response_to_input_items, compose_response_to_input_items_with_budget,
};
pub use record_writer::{
    BestEffortMemoryRecordWriter, MemoryEngineRecordWriter, MemoryRecordScope,
};

use compose_items::default_compose_policy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryScope {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub subject_id: Option<String>,
    pub related_subject_ids: Vec<String>,
    pub policy: Option<ComposeContextPolicy>,
}

impl MemoryScope {
    pub fn thread(
        tenant_id: impl Into<String>,
        source_id: impl Into<String>,
        thread_id: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            source_id: source_id.into(),
            thread_id: thread_id.into(),
            subject_id: None,
            related_subject_ids: Vec::new(),
            policy: None,
        }
    }

    pub fn with_subject_id(mut self, subject_id: impl Into<String>) -> Self {
        self.subject_id = Some(subject_id.into());
        self
    }

    pub fn with_related_subject_ids<I, S>(mut self, related_subject_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.related_subject_ids = related_subject_ids.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_policy(mut self, policy: ComposeContextPolicy) -> Self {
        self.policy = Some(policy);
        self
    }
}

#[derive(Clone)]
pub struct MemoryContextComposer {
    client: MemoryEngineClient,
    source_id: Option<String>,
}

impl MemoryContextComposer {
    pub fn new_direct(
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
    ) -> Result<Self, String> {
        let source_id = source_id.into();
        Ok(Self {
            client: MemoryEngineClient::new_direct(base_url, timeout, source_id.clone())?,
            source_id: Some(source_id),
        })
    }

    pub fn from_client(client: MemoryEngineClient) -> Self {
        Self {
            client,
            source_id: None,
        }
    }

    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    pub async fn compose(&self, scope: &MemoryScope) -> Result<ComposeContextResponse, String> {
        self.validate_scope_source(scope)?;
        self.client
            .compose_context(&SdkComposeContextRequest {
                tenant_id: scope.tenant_id.clone(),
                subject_id: scope.subject_id.clone(),
                related_subject_ids: if scope.related_subject_ids.is_empty() {
                    None
                } else {
                    Some(scope.related_subject_ids.clone())
                },
                thread_id: scope.thread_id.clone(),
                policy: scope.policy.clone().or_else(default_compose_policy),
            })
            .await
    }

    pub async fn compose_input_items(&self, scope: &MemoryScope) -> Result<Vec<Value>, String> {
        self.compose_input_items_with_budget(scope, None).await
    }

    pub async fn compose_input_items_with_budget(
        &self,
        scope: &MemoryScope,
        limits: Option<ToolResultModelBudgetLimits>,
    ) -> Result<Vec<Value>, String> {
        let response = self.compose(scope).await?;
        Ok(compose_response_to_input_items_with_budget(
            &response, limits,
        ))
    }

    pub async fn run_active_summary(
        &self,
        scope: &MemoryScope,
        trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        self.validate_scope_source(scope)?;
        self.client
            .run_thread_active_summary(
                scope.thread_id.as_str(),
                scope.tenant_id.as_str(),
                trigger_reason,
            )
            .await
    }

    pub async fn get_active_summary_status(
        &self,
        scope: &MemoryScope,
        job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        self.validate_scope_source(scope)?;
        self.client
            .get_thread_active_summary_status(
                scope.thread_id.as_str(),
                scope.tenant_id.as_str(),
                job_run_id,
            )
            .await
    }

    pub async fn wait_for_active_summary_completion(
        &self,
        scope: &MemoryScope,
        initial: RunThreadActiveSummaryResponse,
        poll_interval: Duration,
        poll_timeout: Duration,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        if initial.completed || initial.failed || !initial.running {
            return Ok(initial);
        }

        let deadline = Instant::now() + poll_timeout;
        let job_run_id = initial.job_run_id.clone();
        loop {
            if Instant::now() >= deadline {
                return Err(format!(
                    "active summary poll timed out after {} ms",
                    poll_timeout.as_millis()
                ));
            }

            sleep(poll_interval).await;
            let status = self
                .get_active_summary_status(scope, job_run_id.as_deref())
                .await?;
            if status.completed || status.failed || !status.running {
                return Ok(status);
            }
        }
    }

    fn validate_scope_source(&self, scope: &MemoryScope) -> Result<(), String> {
        let Some(client_source_id) = self.source_id.as_deref().and_then(normalized) else {
            return Ok(());
        };
        let Some(scope_source_id) = normalized(scope.source_id.as_str()) else {
            return Ok(());
        };
        if client_source_id == scope_source_id {
            Ok(())
        } else {
            Err(format!(
                "memory scope source_id mismatch: composer={client_source_id}, scope={scope_source_id}"
            ))
        }
    }
}

fn normalized(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
#[path = "memory_context/tests.rs"]
mod tests;
