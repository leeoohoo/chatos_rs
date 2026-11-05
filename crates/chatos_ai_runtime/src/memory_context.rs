// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use memory_engine_sdk::{
    ComposeContextMeta, ComposeContextPolicy, ComposeContextResponse, MemoryEngineClient,
    RunThreadActiveSummaryResponse, SdkComposeContextRequest, SdkUpsertThreadRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{sleep, Instant};
use tracing::warn;

use crate::tool_runtime::ToolResultModelBudgetLimits;
use crate::traits::SaveRecordInput;

pub(crate) const MEMORY_CONTEXT_ITEM_PREFIX: &str = "[memory_engine_context]";

#[path = "memory_context/compose_items.rs"]
mod compose_items;
#[path = "memory_context/log_summary.rs"]
mod log_summary;
#[path = "memory_context/record_writer.rs"]
mod record_writer;

pub use compose_items::{
    compose_response_to_input_items, compose_response_to_input_items_with_budget,
};
pub use record_writer::{MemoryEngineRecordWriter, MemoryRecordScope};

use compose_items::standard_compose_policy;

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
    resilient_cache: Option<Arc<dyn MemoryContextCache>>,
}

#[async_trait]
pub trait MemoryContextCache: Send + Sync {
    async fn load(&self, scope: &MemoryScope) -> Result<Option<ComposeContextResponse>, String>;

    async fn store(
        &self,
        scope: &MemoryScope,
        response: &ComposeContextResponse,
    ) -> Result<(), String>;
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
            resilient_cache: None,
        })
    }

    pub fn from_client(client: MemoryEngineClient) -> Self {
        Self {
            client,
            source_id: None,
            resilient_cache: None,
        }
    }

    pub fn with_resilient_cache<C>(mut self, cache: C) -> Self
    where
        C: MemoryContextCache + 'static,
    {
        self.resilient_cache = Some(Arc::new(cache));
        self
    }

    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    pub async fn compose(&self, scope: &MemoryScope) -> Result<ComposeContextResponse, String> {
        self.validate_scope_source(scope)?;
        let remote = self
            .client
            .compose_context(&SdkComposeContextRequest {
                tenant_id: scope.tenant_id.clone(),
                subject_id: scope.subject_id.clone(),
                related_subject_ids: if scope.related_subject_ids.is_empty() {
                    None
                } else {
                    Some(scope.related_subject_ids.clone())
                },
                thread_id: scope.thread_id.clone(),
                policy: scope.policy.clone().or_else(standard_compose_policy),
            })
            .await;
        let Some(cache) = self.resilient_cache.as_ref() else {
            return remote;
        };
        match remote {
            Ok(response) => {
                if let Err(error) = cache.store(scope, &response).await {
                    warn!(
                        error = error.as_str(),
                        "store local Memory context cache failed"
                    );
                }
                Ok(response)
            }
            Err(remote_error) => match cache.load(scope).await {
                Ok(Some(response)) => {
                    warn!(
                        error = remote_error.as_str(),
                        "retained Memory unavailable; using local context cache"
                    );
                    Ok(response)
                }
                Ok(None) => {
                    warn!(
                        error = remote_error.as_str(),
                        "retained Memory unavailable and no local cache exists; continuing without Memory context"
                    );
                    Ok(empty_context(scope))
                }
                Err(cache_error) => {
                    warn!(
                        error = remote_error.as_str(),
                        cache_error = cache_error.as_str(),
                        "retained Memory and local cache unavailable; continuing without Memory context"
                    );
                    Ok(empty_context(scope))
                }
            },
        }
    }

    /// Establishes the canonical Memory Thread before the first compose call. The current user
    /// message remains part of the live model input and is persisted through the durable outbox;
    /// this call only creates the identity boundary needed to recall an otherwise-new Thread.
    pub async fn ensure_thread_for_record(
        &self,
        scope: &MemoryScope,
        record: &SaveRecordInput,
    ) -> Result<(), String> {
        self.validate_scope_source(scope)?;
        if record.conversation_id.trim() != scope.thread_id {
            return Err("Memory record thread does not match compose scope".to_string());
        }
        let subject_id = scope
            .subject_id
            .as_deref()
            .map(str::trim)
            .filter(|value| value.starts_with("agent:") && value.len() > "agent:".len())
            .ok_or_else(|| "Memory compose scope requires a canonical agent subject".to_string())?;
        let agent_id = subject_id.trim_start_matches("agent:");
        let metadata = record.metadata.as_ref();
        let record_agent_id = metadata_string(metadata, "agent_id")
            .ok_or_else(|| "Memory record requires metadata.agent_id".to_string())?;
        if record_agent_id != agent_id {
            return Err("Memory record agent does not match compose scope".to_string());
        }
        let thread_kind = metadata_string(metadata, "thread_kind")
            .ok_or_else(|| "Memory record requires metadata.thread_kind".to_string())?;
        upsert_canonical_memory_thread(
            &self.client,
            scope.tenant_id.as_str(),
            scope.thread_id.as_str(),
            agent_id,
            metadata_string(metadata, "agent_name").as_deref(),
            metadata_string(metadata, "thread_project_aggregation_id").as_deref(),
            thread_kind.as_str(),
        )
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

    pub async fn compose_input_items_excluding_turn(
        &self,
        scope: &MemoryScope,
        excluded_turn_id: Option<&str>,
        limits: Option<ToolResultModelBudgetLimits>,
    ) -> Result<Vec<Value>, String> {
        let mut response = self.compose(scope).await?;
        if let Some(excluded_turn_id) = excluded_turn_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            response.recent_records.retain(|record| {
                record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("conversation_turn_id"))
                    .and_then(Value::as_str)
                    != Some(excluded_turn_id)
            });
        }
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

pub(crate) async fn upsert_canonical_memory_thread(
    client: &MemoryEngineClient,
    tenant_id: &str,
    thread_id: &str,
    agent_id: &str,
    agent_name: Option<&str>,
    project_aggregation_id: Option<&str>,
    thread_kind: &str,
) -> Result<(), String> {
    let agent_subject = format!("agent:{agent_id}");
    let mut labels = vec![
        "memory-schema:v2".to_string(),
        agent_subject.clone(),
        format!("thread-kind:{thread_kind}"),
    ];
    if let Some(project_id) = project_aggregation_id {
        labels.push(format!("project:{project_id}"));
    }
    client
        .upsert_thread(
            thread_id,
            &SdkUpsertThreadRequest {
                tenant_id: tenant_id.to_string(),
                subject_id: agent_subject,
                thread_type: "conversation".to_string(),
                external_thread_id: Some(thread_id.to_string()),
                title: None,
                labels: Some(labels),
                metadata: Some(serde_json::json!({
                    "agent_id": agent_id,
                    "agent_name": agent_name,
                    "project_aggregation_id": project_aggregation_id,
                    "thread_kind": thread_kind,
                })),
                status: Some("active".to_string()),
                created_at: None,
                updated_at: None,
                archived_at: None,
            },
        )
        .await?;
    Ok(())
}

fn metadata_string(metadata: Option<&Value>, key: &str) -> Option<String> {
    metadata
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn empty_context(scope: &MemoryScope) -> ComposeContextResponse {
    ComposeContextResponse {
        thread_id: scope.thread_id.clone(),
        blocks: Vec::new(),
        recent_records: Vec::new(),
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 0,
        },
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
