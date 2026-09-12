// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Memory Engine backed context composition for providers without a native
//! compaction contract. The adapter never summarizes or truncates locally.

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use chatos_local_agent_protocol::MAX_MODEL_GATEWAY_JSON_BYTES;
use chatos_memory_client::{
    ComposeContextPolicy, ComposeContextRequest, ComposeContextResponse, EngineRecord,
    MemoryEngineClient, RunThreadActiveSummaryResponse,
};
use serde_json::{json, Value};
use tokio::time::{sleep, Instant};
use tokio_util::sync::CancellationToken;

const DEFAULT_RECENT_RECORD_LIMIT: usize = 64;
const DEFAULT_SUMMARY_LIMIT: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEngineContextScope {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub subject_id: Option<String>,
    pub related_subject_ids: Vec<String>,
}

impl MemoryEngineContextScope {
    pub fn thread(
        tenant_id: impl Into<String>,
        source_id: impl Into<String>,
        thread_id: impl Into<String>,
    ) -> Result<Self, MemoryEngineContextError> {
        let scope = Self {
            tenant_id: tenant_id.into(),
            source_id: source_id.into(),
            thread_id: thread_id.into(),
            subject_id: None,
            related_subject_ids: Vec::new(),
        };
        scope.validate()?;
        Ok(scope)
    }

    pub fn with_subject_id(
        mut self,
        subject_id: impl Into<String>,
    ) -> Result<Self, MemoryEngineContextError> {
        self.subject_id = Some(subject_id.into());
        self.validate()?;
        Ok(self)
    }

    pub fn with_related_subject_ids<I, S>(
        mut self,
        related_subject_ids: I,
    ) -> Result<Self, MemoryEngineContextError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.related_subject_ids = related_subject_ids.into_iter().map(Into::into).collect();
        self.validate()?;
        Ok(self)
    }

    fn validate(&self) -> Result<(), MemoryEngineContextError> {
        validate_identity("tenant_id", self.tenant_id.as_str())?;
        validate_identity("source_id", self.source_id.as_str())?;
        validate_identity("thread_id", self.thread_id.as_str())?;
        if let Some(subject_id) = self.subject_id.as_deref() {
            validate_identity("subject_id", subject_id)?;
        }
        for related_subject_id in &self.related_subject_ids {
            validate_identity("related_subject_id", related_subject_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveSummaryWaitPolicy {
    pub poll_interval: Duration,
    pub maximum_wait: Duration,
}

impl ActiveSummaryWaitPolicy {
    pub fn validate(self) -> Result<Self, MemoryEngineContextError> {
        if self.poll_interval.is_zero() || self.maximum_wait.is_zero() {
            return Err(MemoryEngineContextError::InvalidWaitPolicy);
        }
        Ok(self)
    }
}

impl Default for ActiveSummaryWaitPolicy {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            maximum_wait: Duration::from_secs(15 * 60),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedMemoryEngineContext {
    pub input_items: Vec<Value>,
    pub summary_count: usize,
    pub recent_record_count: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MemoryEngineContextError {
    #[error("memory context {field} must be a non-empty identifier")]
    InvalidIdentity { field: &'static str },
    #[error("memory context wait policy durations must be positive")]
    InvalidWaitPolicy,
    #[error("memory context source mismatch: adapter={adapter}, scope={scope}")]
    SourceMismatch { adapter: String, scope: String },
    #[error("memory engine request failed: {detail}")]
    Transport { detail: String },
    #[error("memory engine active summary response is invalid: {reason}")]
    InvalidSummaryStatus { reason: String },
    #[error("memory engine active summary failed: {detail}")]
    SummaryFailed { detail: String },
    #[error("memory engine active summary timed out after {milliseconds} ms")]
    SummaryTimeout { milliseconds: u128 },
    #[error("memory engine context operation was cancelled")]
    Cancelled,
    #[error("memory engine compose response is invalid: {reason}")]
    InvalidComposeResponse { reason: String },
    #[error("memory engine composed context exceeds {maximum} bytes")]
    ContextTooLarge { maximum: usize },
}

#[async_trait]
pub trait MemoryEngineContextApi: Send + Sync {
    async fn compose_context(
        &self,
        request: &ComposeContextRequest,
    ) -> Result<ComposeContextResponse, String>;

    async fn run_active_summary(
        &self,
        thread_id: &str,
        tenant_id: &str,
        trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String>;

    async fn get_active_summary_status(
        &self,
        thread_id: &str,
        tenant_id: &str,
        job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String>;
}

#[async_trait]
impl MemoryEngineContextApi for MemoryEngineClient {
    async fn compose_context(
        &self,
        request: &ComposeContextRequest,
    ) -> Result<ComposeContextResponse, String> {
        MemoryEngineClient::compose_context(self, request).await
    }

    async fn run_active_summary(
        &self,
        thread_id: &str,
        tenant_id: &str,
        trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        self.run_thread_active_summary(thread_id, tenant_id, trigger_reason)
            .await
    }

    async fn get_active_summary_status(
        &self,
        thread_id: &str,
        tenant_id: &str,
        job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        self.get_thread_active_summary_status(thread_id, tenant_id, job_run_id)
            .await
    }
}

#[derive(Clone)]
pub struct MemoryEngineContextAdapter {
    api: Arc<dyn MemoryEngineContextApi>,
    source_id: String,
    wait_policy: ActiveSummaryWaitPolicy,
    compose_policy: ComposeContextPolicy,
}

impl MemoryEngineContextAdapter {
    pub fn from_client(
        client: MemoryEngineClient,
        source_id: impl Into<String>,
    ) -> Result<Self, MemoryEngineContextError> {
        Self::new(Arc::new(client), source_id)
    }

    pub fn new(
        api: Arc<dyn MemoryEngineContextApi>,
        source_id: impl Into<String>,
    ) -> Result<Self, MemoryEngineContextError> {
        let source_id = source_id.into();
        validate_identity("source_id", source_id.as_str())?;
        Ok(Self {
            api,
            source_id,
            wait_policy: ActiveSummaryWaitPolicy::default(),
            compose_policy: ComposeContextPolicy {
                include_recent_records: Some(true),
                include_thread_summary: Some(true),
                include_subject_memory: Some(true),
                recent_record_limit: Some(DEFAULT_RECENT_RECORD_LIMIT),
                summary_limit: Some(DEFAULT_SUMMARY_LIMIT),
            },
        })
    }

    pub fn source_id(&self) -> &str {
        self.source_id.as_str()
    }

    pub fn with_wait_policy(
        mut self,
        wait_policy: ActiveSummaryWaitPolicy,
    ) -> Result<Self, MemoryEngineContextError> {
        self.wait_policy = wait_policy.validate()?;
        Ok(self)
    }

    pub fn with_compose_policy(mut self, compose_policy: ComposeContextPolicy) -> Self {
        self.compose_policy = compose_policy;
        self
    }

    /// Waits for an already-running summary and then composes the latest
    /// authoritative Memory Engine context.
    pub async fn prepare_model_input(
        &self,
        scope: &MemoryEngineContextScope,
        cancellation: &CancellationToken,
    ) -> Result<PreparedMemoryEngineContext, MemoryEngineContextError> {
        self.validate_scope(scope)?;
        if cancellation.is_cancelled() {
            return Err(MemoryEngineContextError::Cancelled);
        }
        let status = tokio::select! {
            () = cancellation.cancelled() => {
                return Err(MemoryEngineContextError::Cancelled);
            }
            result = self.api.get_active_summary_status(
                scope.thread_id.as_str(),
                scope.tenant_id.as_str(),
                None,
            ) => result.map_err(transport_error)?,
        };
        self.wait_for_summary(scope, status, cancellation).await?;
        self.compose(scope, cancellation).await
    }

    /// Starts one server-managed summary and composes again after it reaches a
    /// formal terminal state. The caller decides whether token pressure
    /// requires this operation; this adapter never invents a local summary.
    pub async fn summarize_and_prepare(
        &self,
        scope: &MemoryEngineContextScope,
        trigger_reason: Option<&str>,
        cancellation: &CancellationToken,
    ) -> Result<PreparedMemoryEngineContext, MemoryEngineContextError> {
        self.validate_scope(scope)?;
        if cancellation.is_cancelled() {
            return Err(MemoryEngineContextError::Cancelled);
        }
        let status = tokio::select! {
            () = cancellation.cancelled() => {
                return Err(MemoryEngineContextError::Cancelled);
            }
            result = self.api.run_active_summary(
                scope.thread_id.as_str(),
                scope.tenant_id.as_str(),
                trigger_reason,
            ) => result.map_err(transport_error)?,
        };
        self.wait_for_summary(scope, status, cancellation).await?;
        self.compose(scope, cancellation).await
    }

    fn validate_scope(
        &self,
        scope: &MemoryEngineContextScope,
    ) -> Result<(), MemoryEngineContextError> {
        scope.validate()?;
        if scope.source_id != self.source_id {
            return Err(MemoryEngineContextError::SourceMismatch {
                adapter: self.source_id.clone(),
                scope: scope.source_id.clone(),
            });
        }
        Ok(())
    }

    async fn wait_for_summary(
        &self,
        scope: &MemoryEngineContextScope,
        mut status: RunThreadActiveSummaryResponse,
        cancellation: &CancellationToken,
    ) -> Result<RunThreadActiveSummaryResponse, MemoryEngineContextError> {
        let deadline = Instant::now() + self.wait_policy.maximum_wait;
        loop {
            validate_summary_status(scope, &status)?;
            if status.failed {
                return Err(MemoryEngineContextError::SummaryFailed {
                    detail: status
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "server reported failure without detail".to_string()),
                });
            }
            if !status.running {
                return Ok(status);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(MemoryEngineContextError::SummaryTimeout {
                    milliseconds: self.wait_policy.maximum_wait.as_millis(),
                });
            }
            let delay = self
                .wait_policy
                .poll_interval
                .min(deadline.saturating_duration_since(now));
            tokio::select! {
                () = cancellation.cancelled() => {
                    return Err(MemoryEngineContextError::Cancelled);
                }
                () = sleep(delay) => {}
            }
            status = tokio::select! {
                () = cancellation.cancelled() => {
                    return Err(MemoryEngineContextError::Cancelled);
                }
                result = self.api.get_active_summary_status(
                    scope.thread_id.as_str(),
                    scope.tenant_id.as_str(),
                    status.job_run_id.as_deref(),
                ) => result.map_err(transport_error)?,
            };
        }
    }

    async fn compose(
        &self,
        scope: &MemoryEngineContextScope,
        cancellation: &CancellationToken,
    ) -> Result<PreparedMemoryEngineContext, MemoryEngineContextError> {
        if cancellation.is_cancelled() {
            return Err(MemoryEngineContextError::Cancelled);
        }
        let request = ComposeContextRequest {
            tenant_id: scope.tenant_id.clone(),
            subject_id: scope.subject_id.clone(),
            related_subject_ids: (!scope.related_subject_ids.is_empty())
                .then(|| scope.related_subject_ids.clone()),
            thread_id: scope.thread_id.clone(),
            policy: Some(self.compose_policy.clone()),
        };
        let response = tokio::select! {
            () = cancellation.cancelled() => {
                return Err(MemoryEngineContextError::Cancelled);
            }
            result = self.api.compose_context(&request) => result.map_err(transport_error)?,
        };
        validate_compose_response(scope, &response)?;
        let input_items = compose_response_items(&response)?;
        if serde_json::to_vec(&input_items)
            .map_err(|error| MemoryEngineContextError::InvalidComposeResponse {
                reason: error.to_string(),
            })?
            .len()
            > MAX_MODEL_GATEWAY_JSON_BYTES
        {
            return Err(MemoryEngineContextError::ContextTooLarge {
                maximum: MAX_MODEL_GATEWAY_JSON_BYTES,
            });
        }
        Ok(PreparedMemoryEngineContext {
            input_items,
            summary_count: response.meta.summary_count,
            recent_record_count: response.meta.recent_record_count,
        })
    }
}

fn validate_identity(field: &'static str, value: &str) -> Result<(), MemoryEngineContextError> {
    if value.trim().is_empty() || value != value.trim() || value.len() > 512 {
        Err(MemoryEngineContextError::InvalidIdentity { field })
    } else {
        Ok(())
    }
}

fn transport_error(detail: String) -> MemoryEngineContextError {
    MemoryEngineContextError::Transport { detail }
}

fn validate_summary_status(
    scope: &MemoryEngineContextScope,
    status: &RunThreadActiveSummaryResponse,
) -> Result<(), MemoryEngineContextError> {
    if status.thread_id != scope.thread_id {
        return Err(MemoryEngineContextError::InvalidSummaryStatus {
            reason: format!(
                "thread mismatch: expected {}, received {}",
                scope.thread_id, status.thread_id
            ),
        });
    }
    // The service marks failed jobs as both completed and failed. Running is
    // the only state that must never overlap a terminal flag.
    if status.running && (status.completed || status.failed) {
        return Err(MemoryEngineContextError::InvalidSummaryStatus {
            reason: "a running summary cannot also be completed or failed".to_string(),
        });
    }
    if status.running
        && status
            .job_run_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(MemoryEngineContextError::InvalidSummaryStatus {
            reason: "a running summary must include job_run_id".to_string(),
        });
    }
    Ok(())
}

fn validate_compose_response(
    scope: &MemoryEngineContextScope,
    response: &ComposeContextResponse,
) -> Result<(), MemoryEngineContextError> {
    if response.thread_id != scope.thread_id {
        return invalid_compose(format!(
            "thread mismatch: expected {}, received {}",
            scope.thread_id, response.thread_id
        ));
    }
    // `summary_count` counts source summary rows while one block can contain
    // multiple rolled-up rows and subject-memory blocks are not summaries.
    // Only the recent-record count has a one-to-one payload relationship.
    if response.meta.recent_record_count != response.recent_records.len() {
        return invalid_compose(
            "recent-record metadata count does not match the payload".to_string(),
        );
    }
    for block in &response.blocks {
        if block.block_type.trim().is_empty() || block.text.trim().is_empty() {
            return invalid_compose("summary blocks must include a type and text".to_string());
        }
    }
    for record in &response.recent_records {
        if record.thread_id != scope.thread_id
            || record.tenant_id != scope.tenant_id
            || record.source_id != scope.source_id
        {
            return invalid_compose(format!(
                "record {} is outside the requested tenant/source/thread scope",
                record.id
            ));
        }
    }
    Ok(())
}

fn compose_response_items(
    response: &ComposeContextResponse,
) -> Result<Vec<Value>, MemoryEngineContextError> {
    let mut items = Vec::new();
    if !response.blocks.is_empty() {
        let text = response
            .blocks
            .iter()
            .map(|block| format!("[{}]\n{}", block.block_type, block.text))
            .collect::<Vec<_>>()
            .join("\n\n===\n\n");
        items.push(message_item("system", text.as_str()));
    }

    let paired_calls = paired_tool_call_ids(response.recent_records.as_slice())?;
    let mut emitted_calls = HashMap::<String, ()>::new();
    for record in &response.recent_records {
        match record.role.trim() {
            "user" | "system" | "developer" => {
                if !record.content.trim().is_empty() {
                    items.push(message_item(record.role.trim(), record.content.as_str()));
                }
            }
            "assistant" => {
                if !record.content.trim().is_empty() {
                    items.push(assistant_message_item(record.content.as_str()));
                }
                for call in extract_tool_calls(record) {
                    let Some(call_id) = tool_call_id(call) else {
                        continue;
                    };
                    if !paired_calls.contains_key(call_id) {
                        continue;
                    }
                    let name = tool_call_name(call).ok_or_else(|| {
                        MemoryEngineContextError::InvalidComposeResponse {
                            reason: format!("paired tool call {call_id} has no name"),
                        }
                    })?;
                    let arguments = tool_call_arguments(call);
                    items.push(json!({
                        "type": "function_call",
                        "call_id": call_id,
                        "name": name,
                        "arguments": arguments,
                    }));
                    emitted_calls.insert(call_id.to_string(), ());
                }
            }
            "tool" => {
                let Some(call_id) = record_tool_call_id(record) else {
                    return invalid_compose(format!(
                        "tool record {} has no tool_call_id",
                        record.id
                    ));
                };
                if emitted_calls.contains_key(call_id) {
                    items.push(json!({
                        "type": "function_call_output",
                        "call_id": call_id,
                        "output": record.content,
                    }));
                }
            }
            role => {
                return invalid_compose(format!(
                    "record {} has unsupported role {role}",
                    record.id
                ));
            }
        }
    }
    Ok(items)
}

fn paired_tool_call_ids(
    records: &[EngineRecord],
) -> Result<HashMap<String, ()>, MemoryEngineContextError> {
    let mut calls = HashMap::<String, usize>::new();
    let mut outputs = HashMap::<String, usize>::new();
    for record in records {
        if record.role.trim() == "assistant" {
            for call in extract_tool_calls(record) {
                if let Some(call_id) = tool_call_id(call) {
                    *calls.entry(call_id.to_string()).or_default() += 1;
                }
            }
        } else if record.role.trim() == "tool" {
            if let Some(call_id) = record_tool_call_id(record) {
                *outputs.entry(call_id.to_string()).or_default() += 1;
            }
        }
    }
    for (call_id, count) in calls.iter().chain(outputs.iter()) {
        if *count > 1 {
            return invalid_compose(format!(
                "tool call id {call_id} occurs more than once in composed records"
            ));
        }
    }
    Ok(calls
        .into_keys()
        .filter(|call_id| outputs.contains_key(call_id))
        .map(|call_id| (call_id, ()))
        .collect())
}

fn extract_tool_calls(record: &EngineRecord) -> Vec<&Value> {
    [record.structured_payload.as_ref(), record.metadata.as_ref()]
        .into_iter()
        .flatten()
        .find_map(|container| {
            container
                .get("tool_calls")
                .or_else(|| container.get("toolCalls"))
                .and_then(Value::as_array)
        })
        .map(|calls| calls.iter().collect())
        .unwrap_or_default()
}

fn tool_call_id(call: &Value) -> Option<&str> {
    ["call_id", "id", "tool_call_id", "toolCallId"]
        .into_iter()
        .find_map(|key| call.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn tool_call_name(call: &Value) -> Option<&str> {
    call.get("function")
        .and_then(|function| function.get("name"))
        .and_then(Value::as_str)
        .or_else(|| call.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn tool_call_arguments(call: &Value) -> String {
    let value = call
        .get("function")
        .and_then(|function| function.get("arguments"))
        .or_else(|| call.get("arguments"));
    match value {
        Some(Value::String(arguments)) => arguments.clone(),
        Some(arguments) => arguments.to_string(),
        None => "{}".to_string(),
    }
}

fn record_tool_call_id(record: &EngineRecord) -> Option<&str> {
    [record.metadata.as_ref(), record.structured_payload.as_ref()]
        .into_iter()
        .flatten()
        .find_map(|container| {
            ["tool_call_id", "toolCallId", "call_id"]
                .into_iter()
                .find_map(|key| container.get(key).and_then(Value::as_str))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn message_item(role: &str, text: &str) -> Value {
    json!({
        "type": "message",
        "role": role,
        "content": [{"type": "input_text", "text": text}],
    })
}

fn assistant_message_item(text: &str) -> Value {
    json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "output_text", "text": text}],
    })
}

fn invalid_compose<T>(reason: String) -> Result<T, MemoryEngineContextError> {
    Err(MemoryEngineContextError::InvalidComposeResponse { reason })
}
