// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use memory_engine_sdk::{
    ComposeContextPolicy, ComposeContextResponse, MemoryEngineClient,
    RunThreadActiveSummaryResponse, RuntimeContextAssertionIssuer, SdkComposeContextRequest,
    SdkUpsertThreadRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::sleep;
use tracing::warn;

use crate::tool_runtime::ToolResultModelBudgetLimits;
use crate::traits::SaveRecordInput;

pub(crate) const MEMORY_CONTEXT_ITEM_PREFIX: &str = "[memory_engine_context]";
const REQUIRED_COMPACTION_POLL_INTERVAL: Duration = Duration::from_millis(250);
const MAX_REQUIRED_COMPACTION_ROUNDS: usize = 128;

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

#[derive(Debug, Clone, Copy)]
pub struct RuntimeContextAssertionRequest<'a> {
    pub principal_id: &'a str,
    pub tenant_id: &'a str,
    pub source_id: &'a str,
    pub canonical_agent_subject_id: &'a str,
    pub thread_id: &'a str,
    pub project_partition_key: &'a str,
    pub operation: &'a str,
}

#[async_trait]
pub trait RuntimeContextAssertionProvider: Send + Sync {
    async fn issue(&self, request: RuntimeContextAssertionRequest<'_>) -> Result<String, String>;
}

#[async_trait]
impl RuntimeContextAssertionProvider for RuntimeContextAssertionIssuer {
    async fn issue(&self, request: RuntimeContextAssertionRequest<'_>) -> Result<String, String> {
        self.issue(
            request.principal_id,
            request.tenant_id,
            request.source_id,
            request.canonical_agent_subject_id,
            request.thread_id,
            request.project_partition_key,
            &[request.operation],
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryScope {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub subject_id: Option<String>,
    pub context_project_subject_id: Option<String>,
    pub query_text: Option<String>,
    pub current_turn_id: Option<String>,
    pub model_capability_revision: String,
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
            context_project_subject_id: None,
            query_text: None,
            current_turn_id: None,
            model_capability_revision: String::new(),
            policy: None,
        }
    }

    pub fn with_subject_id(mut self, subject_id: impl Into<String>) -> Self {
        self.subject_id = Some(subject_id.into());
        self
    }

    pub fn with_context_project_subject_id(
        mut self,
        context_project_subject_id: impl Into<String>,
    ) -> Self {
        self.context_project_subject_id = Some(context_project_subject_id.into());
        self
    }

    pub fn project_partition_key(&self) -> Result<String, String> {
        let Some(value) = self
            .context_project_subject_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok("__none__".to_string());
        };
        let normalized = value.strip_prefix("project:").unwrap_or(value);
        if normalized.is_empty() || normalized.chars().any(char::is_control) {
            return Err("memory context project identity is invalid".to_string());
        }
        Ok(format!("project:{normalized}"))
    }

    pub fn with_policy(mut self, policy: ComposeContextPolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    pub fn with_compose_context(
        mut self,
        query_text: Option<String>,
        current_turn_id: Option<String>,
        model_capability_revision: impl Into<String>,
    ) -> Self {
        self.query_text = query_text.and_then(|value| {
            let value = value.trim().to_string();
            (!value.is_empty()).then_some(value)
        });
        self.current_turn_id = current_turn_id.and_then(|value| {
            let value = value.trim().to_string();
            (!value.is_empty()).then_some(value)
        });
        self.model_capability_revision = model_capability_revision.into();
        self
    }
}

#[derive(Clone)]
pub struct MemoryContextComposer {
    client: MemoryEngineClient,
    source_id: Option<String>,
    assertion_provider: Option<Arc<dyn RuntimeContextAssertionProvider>>,
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
            assertion_provider: None,
            resilient_cache: None,
        })
    }

    pub fn from_client(client: MemoryEngineClient) -> Self {
        let source_id = client.source_id().map(str::to_string);
        Self {
            client,
            source_id,
            assertion_provider: None,
            resilient_cache: None,
        }
    }

    pub fn with_runtime_context_assertion_issuer(
        self,
        assertion_issuer: RuntimeContextAssertionIssuer,
    ) -> Self {
        self.with_runtime_context_assertion_provider(assertion_issuer)
    }

    pub fn with_runtime_context_assertion_provider<P>(mut self, assertion_provider: P) -> Self
    where
        P: RuntimeContextAssertionProvider + 'static,
    {
        self.assertion_provider = Some(Arc::new(assertion_provider));
        self
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
        let mut response = self.compose_once(scope).await?;
        for _ in 0..MAX_REQUIRED_COMPACTION_ROUNDS {
            if !response.meta.needs_compaction {
                if let Some(cache) = self.resilient_cache.as_ref() {
                    if let Err(error) = cache.store(scope, &response).await {
                        warn!(
                            error = error.as_str(),
                            "store local Memory context cache failed"
                        );
                    }
                }
                return Ok(response);
            }
            let before_pending = response
                .meta
                .candidate_count_by_pool
                .get("raw_tail")
                .copied()
                .unwrap_or(response.meta.recent_record_count);
            let before_tokens = response.meta.budget_used.total;
            let initial = self
                .run_active_summary(scope, Some("compose_budget_compaction"))
                .await?;
            let status = self
                .wait_for_active_summary_completion(
                    scope,
                    initial,
                    REQUIRED_COMPACTION_POLL_INTERVAL,
                )
                .await?;
            if status.failed || (!status.generated && !status.compacted) {
                return Err(
                    "Memory compose required compaction but the summary job made no progress"
                        .to_string(),
                );
            }
            let recomposed = self.compose_once(scope).await?;
            let after_pending = recomposed
                .meta
                .candidate_count_by_pool
                .get("raw_tail")
                .copied()
                .unwrap_or(recomposed.meta.recent_record_count);
            if after_pending >= before_pending && recomposed.meta.budget_used.total >= before_tokens
            {
                return Err(
                    "Memory compose compaction did not reduce pending records or token usage"
                        .to_string(),
                );
            }
            response = recomposed;
        }
        Err("Memory compose exceeded the required compaction round limit".to_string())
    }

    async fn compose_once(&self, scope: &MemoryScope) -> Result<ComposeContextResponse, String> {
        self.validate_scope_source(scope)?;
        let runtime_context_assertion = self.issue_assertion(scope, "compose").await?;
        self.client
            .compose_context(&SdkComposeContextRequest {
                tenant_id: scope.tenant_id.clone(),
                subject_id: scope.subject_id.clone(),
                context_project_subject_id: scope.context_project_subject_id.clone(),
                thread_id: scope.thread_id.clone(),
                query_text: scope.query_text.clone(),
                current_turn_id: scope.current_turn_id.clone(),
                model_capability_revision: scope.model_capability_revision.clone(),
                policy: scope.policy.clone().ok_or_else(|| {
                    "Memory compose requires an explicit model-derived request budget".to_string()
                })?,
                runtime_context_assertion,
            })
            .await
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
        let project_partition_key = scope.project_partition_key()?;
        let runtime_context_assertion = self.issue_assertion(scope, "summarize").await?;
        self.client
            .run_thread_active_summary(
                scope.thread_id.as_str(),
                scope.tenant_id.as_str(),
                project_partition_key.as_str(),
                runtime_context_assertion.as_str(),
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
        let project_partition_key = scope.project_partition_key()?;
        let runtime_context_assertion = self.issue_assertion(scope, "summarize").await?;
        self.client
            .get_thread_active_summary_status(
                scope.thread_id.as_str(),
                scope.tenant_id.as_str(),
                project_partition_key.as_str(),
                runtime_context_assertion.as_str(),
                job_run_id,
            )
            .await
    }

    pub async fn wait_for_active_summary_completion(
        &self,
        scope: &MemoryScope,
        initial: RunThreadActiveSummaryResponse,
        poll_interval: Duration,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        // The Memory service owns the job deadline and lease recovery. A successful status
        // response with running=true is progress, even when summarization takes minutes.
        wait_for_summary_job(initial, poll_interval, |job_run_id| async move {
            self.get_active_summary_status(scope, job_run_id.as_deref())
                .await
        })
        .await
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

    async fn issue_assertion(
        &self,
        scope: &MemoryScope,
        operation: &str,
    ) -> Result<String, String> {
        let provider = self.assertion_provider.as_ref().ok_or_else(|| {
            "Memory runtime context assertion provider is not configured".to_string()
        })?;
        let canonical_agent_subject_id = scope
            .subject_id
            .as_deref()
            .map(str::trim)
            .filter(|value| value.starts_with("agent:") && value.len() > "agent:".len())
            .ok_or_else(|| "Memory scope requires a canonical agent subject".to_string())?;
        let project_partition_key = scope.project_partition_key()?;
        provider
            .issue(RuntimeContextAssertionRequest {
                principal_id: scope.tenant_id.as_str(),
                tenant_id: scope.tenant_id.as_str(),
                source_id: scope.source_id.as_str(),
                canonical_agent_subject_id,
                thread_id: scope.thread_id.as_str(),
                project_partition_key: project_partition_key.as_str(),
                operation,
            })
            .await
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

fn normalized(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

async fn wait_for_summary_job<F, Fut>(
    mut status: RunThreadActiveSummaryResponse,
    poll_interval: Duration,
    mut poll: F,
) -> Result<RunThreadActiveSummaryResponse, String>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<RunThreadActiveSummaryResponse, String>>,
{
    let job_run_id = status.job_run_id.clone();
    while status.running && !status.completed && !status.failed {
        sleep(poll_interval).await;
        status = poll(job_run_id.clone()).await?;
    }
    Ok(status)
}

#[cfg(test)]
#[path = "memory_context/tests.rs"]
mod tests;
