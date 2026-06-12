use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use memory_engine_sdk::{
    ComposeContextPolicy, ComposeContextResponse, EngineRecord, MemoryEngineClient,
    RunThreadActiveSummaryResponse, SdkBatchSyncRecordsRequest, SdkComposeContextRequest,
    UpsertRecordInput,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{sleep, Instant};
use tracing::{info, warn};
use uuid::Uuid;

use crate::input_transform::to_message_item;
use crate::tool_call::{
    build_function_call_item, build_function_call_output_item, extract_message_tool_calls,
    extract_tool_call_id, extract_tool_call_name, tool_call_arguments_text,
};
use crate::tool_runtime::{ToolResultModelBudget, ToolResultModelBudgetLimits};
use crate::traits::{
    MemoryRecordWriter, SaveAssistantRecordInput, SaveRecordInput, SaveToolRecordInput,
};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecordScope {
    pub tenant_id: String,
    pub thread_id: Option<String>,
    pub record_type: String,
    pub default_summary_status: Option<String>,
}

impl MemoryRecordScope {
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            thread_id: None,
            record_type: "message".to_string(),
            default_summary_status: Some("pending".to_string()),
        }
    }

    pub fn message_thread(tenant_id: impl Into<String>, thread_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            thread_id: Some(thread_id.into()),
            record_type: "message".to_string(),
            default_summary_status: Some("pending".to_string()),
        }
    }

    pub fn with_thread_id(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    pub fn with_record_type(mut self, record_type: impl Into<String>) -> Self {
        self.record_type = record_type.into();
        self
    }

    pub fn with_default_summary_status(mut self, default_summary_status: Option<String>) -> Self {
        self.default_summary_status = default_summary_status;
        self
    }
}

#[derive(Clone)]
pub struct MemoryContextComposer {
    client: MemoryEngineClient,
    source_id: Option<String>,
}

#[derive(Clone)]
pub struct MemoryEngineRecordWriter {
    client: MemoryEngineClient,
    scope: MemoryRecordScope,
    source_id: Option<String>,
}

#[derive(Clone)]
pub struct BestEffortMemoryRecordWriter {
    inner: Arc<dyn MemoryRecordWriter>,
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

impl MemoryEngineRecordWriter {
    pub fn new_direct(
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
        scope: MemoryRecordScope,
    ) -> Result<Self, String> {
        let source_id = source_id.into();
        Ok(Self {
            client: MemoryEngineClient::new_direct(base_url, timeout, source_id.clone())?,
            scope,
            source_id: Some(source_id),
        })
    }

    pub fn from_client(client: MemoryEngineClient, scope: MemoryRecordScope) -> Self {
        Self {
            client,
            scope,
            source_id: None,
        }
    }

    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }
}

impl BestEffortMemoryRecordWriter {
    pub fn new<T>(inner: T) -> Self
    where
        T: MemoryRecordWriter + 'static,
    {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn from_arc(inner: Arc<dyn MemoryRecordWriter>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl MemoryRecordWriter for MemoryEngineRecordWriter {
    async fn save_record(&self, input: SaveRecordInput) -> Result<(), String> {
        let tenant_id = self.tenant_id()?;
        let thread_id = self.thread_id_for_record(&input)?;
        let record = self.upsert_record_input(input)?;
        let records = vec![record];
        let summary = summarize_record_batch(records.as_slice());
        let source_id = self.source_id.as_deref().unwrap_or("");
        info!(
            tenant_id = tenant_id.as_str(),
            source_id,
            thread_id = thread_id.as_str(),
            record_count = summary.record_count,
            record_roles = summary.roles.as_str(),
            record_ids = summary.record_ids.as_str(),
            tool_names = summary.tool_names.as_str(),
            content_bytes = summary.content_bytes,
            max_content_bytes = summary.max_content_bytes,
            metadata_bytes = summary.metadata_bytes,
            structured_payload_bytes = summary.structured_payload_bytes,
            "memory engine record batch sync start"
        );
        let response = self
            .client
            .batch_sync_records(
                thread_id.as_str(),
                &SdkBatchSyncRecordsRequest { tenant_id, records },
            )
            .await;
        match &response {
            Ok(response) => {
                info!(
                    source_id,
                    thread_id = thread_id.as_str(),
                    received_count = response.received_count,
                    upserted_count = response.upserted_count,
                    "memory engine record batch sync completed"
                );
            }
            Err(err) => {
                warn!(
                    source_id,
                    thread_id = thread_id.as_str(),
                    record_count = summary.record_count,
                    record_roles = summary.roles.as_str(),
                    record_ids = summary.record_ids.as_str(),
                    tool_names = summary.tool_names.as_str(),
                    content_bytes = summary.content_bytes,
                    max_content_bytes = summary.max_content_bytes,
                    metadata_bytes = summary.metadata_bytes,
                    structured_payload_bytes = summary.structured_payload_bytes,
                    error = err.as_str(),
                    "memory engine record batch sync failed"
                );
            }
        }
        response?;
        Ok(())
    }

    async fn save_tool_records(&self, inputs: Vec<SaveToolRecordInput>) -> Result<(), String> {
        if inputs.is_empty() {
            return Ok(());
        }

        let tenant_id = self.tenant_id()?;
        let mut batches: BTreeMap<String, Vec<UpsertRecordInput>> = BTreeMap::new();
        for input in inputs {
            let input: SaveRecordInput = input.into();
            let thread_id = self.thread_id_for_record(&input)?;
            let record = self.upsert_record_input(input)?;
            batches.entry(thread_id).or_default().push(record);
        }

        for (thread_id, records) in batches {
            let summary = summarize_record_batch(records.as_slice());
            let source_id = self.source_id.as_deref().unwrap_or("");
            info!(
                tenant_id = tenant_id.as_str(),
                source_id,
                thread_id = thread_id.as_str(),
                record_count = summary.record_count,
                record_roles = summary.roles.as_str(),
                record_ids = summary.record_ids.as_str(),
                tool_names = summary.tool_names.as_str(),
                content_bytes = summary.content_bytes,
                max_content_bytes = summary.max_content_bytes,
                metadata_bytes = summary.metadata_bytes,
                structured_payload_bytes = summary.structured_payload_bytes,
                "memory engine tool record batch sync start"
            );
            let response = self
                .client
                .batch_sync_records(
                    thread_id.as_str(),
                    &SdkBatchSyncRecordsRequest {
                        tenant_id: tenant_id.clone(),
                        records,
                    },
                )
                .await;
            match &response {
                Ok(response) => {
                    info!(
                        source_id,
                        thread_id = thread_id.as_str(),
                        received_count = response.received_count,
                        upserted_count = response.upserted_count,
                        "memory engine tool record batch sync completed"
                    );
                }
                Err(err) => {
                    warn!(
                        source_id,
                        thread_id = thread_id.as_str(),
                        record_count = summary.record_count,
                        record_roles = summary.roles.as_str(),
                        record_ids = summary.record_ids.as_str(),
                        tool_names = summary.tool_names.as_str(),
                        content_bytes = summary.content_bytes,
                        max_content_bytes = summary.max_content_bytes,
                        metadata_bytes = summary.metadata_bytes,
                        structured_payload_bytes = summary.structured_payload_bytes,
                        error = err.as_str(),
                        "memory engine tool record batch sync failed"
                    );
                }
            }
            response?;
        }

        Ok(())
    }
}

#[async_trait]
impl MemoryRecordWriter for BestEffortMemoryRecordWriter {
    async fn save_record(&self, input: SaveRecordInput) -> Result<(), String> {
        let summary = summarize_save_record_input(&input);
        if let Err(err) = self.inner.save_record(input).await {
            warn!(
                role = summary.role.as_str(),
                conversation_id = summary.conversation_id.as_str(),
                conversation_turn_id = summary.conversation_turn_id.as_str(),
                message_id = summary.message_id.as_str(),
                tool_call_id = summary.tool_call_id.as_str(),
                content_bytes = summary.content_bytes,
                error = err.as_str(),
                "best-effort memory record sync failed"
            );
        }
        Ok(())
    }

    async fn save_assistant_record(&self, input: SaveAssistantRecordInput) -> Result<(), String> {
        let summary = summarize_assistant_record_input(&input);
        if let Err(err) = self.inner.save_assistant_record(input).await {
            warn!(
                conversation_id = summary.conversation_id.as_str(),
                conversation_turn_id = summary.conversation_turn_id.as_str(),
                message_id = summary.message_id.as_str(),
                response_id = summary.response_id.as_str(),
                response_status = summary.response_status.as_str(),
                content_bytes = summary.content_bytes,
                error = err.as_str(),
                "best-effort memory assistant record sync failed"
            );
        }
        Ok(())
    }

    async fn save_tool_record(&self, input: SaveToolRecordInput) -> Result<(), String> {
        let summary = summarize_tool_record_input(&input);
        if let Err(err) = self.inner.save_tool_record(input).await {
            warn!(
                conversation_id = summary.conversation_id.as_str(),
                conversation_turn_id = summary.conversation_turn_id.as_str(),
                message_id = summary.message_id.as_str(),
                tool_call_id = summary.tool_call_id.as_str(),
                tool_name = summary.tool_name.as_str(),
                content_bytes = summary.content_bytes,
                error = err.as_str(),
                "best-effort memory tool record sync failed"
            );
        }
        Ok(())
    }

    async fn save_tool_records(&self, inputs: Vec<SaveToolRecordInput>) -> Result<(), String> {
        let summary = summarize_tool_record_inputs(inputs.as_slice());
        if let Err(err) = self.inner.save_tool_records(inputs).await {
            warn!(
                record_count = summary.record_count,
                conversation_ids = summary.conversation_ids.as_str(),
                conversation_turn_ids = summary.conversation_turn_ids.as_str(),
                tool_names = summary.tool_names.as_str(),
                tool_call_ids = summary.tool_call_ids.as_str(),
                content_bytes = summary.content_bytes,
                max_content_bytes = summary.max_content_bytes,
                error = err.as_str(),
                "best-effort memory tool records sync failed"
            );
        }
        Ok(())
    }
}

impl MemoryEngineRecordWriter {
    fn tenant_id(&self) -> Result<String, String> {
        normalized(self.scope.tenant_id.as_str())
            .ok_or_else(|| "memory record tenant_id is required".to_string())
    }

    fn thread_id_for_record(&self, input: &SaveRecordInput) -> Result<String, String> {
        self.scope
            .thread_id
            .as_deref()
            .and_then(normalized)
            .or_else(|| normalized(input.conversation_id.as_str()))
            .ok_or_else(|| "memory record thread_id is required".to_string())
    }

    fn upsert_record_input(&self, input: SaveRecordInput) -> Result<UpsertRecordInput, String> {
        let role = normalized(input.role.as_str())
            .ok_or_else(|| "memory record role is required".to_string())?;
        let record_type =
            normalized(self.scope.record_type.as_str()).unwrap_or_else(|| "message".to_string());
        let metadata = input.packed_metadata();
        Ok(UpsertRecordInput {
            id: input
                .message_id
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            external_record_id: None,
            role,
            record_type,
            content: input.content,
            structured_payload: input.structured_payload,
            metadata,
            summary_status: input
                .summary_status
                .or_else(|| self.scope.default_summary_status.clone()),
            summary_id: input.summary_id,
            summarized_at: input.summarized_at,
            created_at: input
                .created_at
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        })
    }
}

pub fn compose_response_to_input_items(response: &ComposeContextResponse) -> Vec<Value> {
    compose_response_to_input_items_with_budget(response, None)
}

pub fn compose_response_to_input_items_with_budget(
    response: &ComposeContextResponse,
    limits: Option<ToolResultModelBudgetLimits>,
) -> Vec<Value> {
    let mut items = Vec::new();
    let mut seen_tool_call_ids = HashSet::new();
    let mut remaining_tool_output_ids = collect_tool_output_id_counts(&response.recent_records);
    let mut tool_result_budget = limits
        .map(ToolResultModelBudget::from_limits)
        .unwrap_or_else(ToolResultModelBudget::from_env);

    if !response.blocks.is_empty() {
        let text = response
            .blocks
            .iter()
            .map(|block| format!("[{}]\n{}", block.block_type, block.text))
            .collect::<Vec<_>>()
            .join("\n\n===\n\n");
        if !text.trim().is_empty() {
            items.push(to_message_item("system", &Value::String(text), false));
        }
    }

    for record in &response.recent_records {
        items.extend(engine_record_to_input_items(
            record,
            &mut seen_tool_call_ids,
            &mut remaining_tool_output_ids,
            &mut tool_result_budget,
        ));
    }

    items
}

fn default_compose_policy() -> Option<ComposeContextPolicy> {
    Some(ComposeContextPolicy {
        include_recent_records: Some(true),
        include_thread_summary: Some(true),
        include_subject_memory: Some(true),
        recent_record_limit: None,
        summary_limit: None,
    })
}

fn engine_record_to_input_items(
    record: &EngineRecord,
    seen_tool_call_ids: &mut HashSet<String>,
    remaining_tool_output_ids: &mut HashMap<String, usize>,
    tool_result_budget: &mut ToolResultModelBudget,
) -> Vec<Value> {
    let role = record.role.trim();
    if role.is_empty() {
        return Vec::new();
    }
    let mut items = Vec::new();

    if role == "tool" {
        if let Some(tool_call_id) = engine_record_tool_call_id(record) {
            if seen_tool_call_ids.contains(tool_call_id.as_str()) {
                let tool_name = record
                    .metadata
                    .as_ref()
                    .and_then(|value| {
                        value
                            .get("name")
                            .or_else(|| value.get("tool_name"))
                            .or_else(|| value.get("toolName"))
                    })
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let output =
                    tool_result_budget.sanitize_content(tool_name, record.content.as_str());
                items.push(build_function_call_output_item(
                    tool_call_id.as_str(),
                    output.as_str(),
                ));
            }
            decrement_remaining_tool_output_id(remaining_tool_output_ids, tool_call_id.as_str());
        }
        return items;
    }

    if role == "assistant" {
        if !record.content.trim().is_empty() {
            items.push(to_message_item(
                "assistant",
                &Value::String(record.content.clone()),
                false,
            ));
        }
        for tool_call in
            extract_message_tool_calls(record.structured_payload.as_ref(), record.metadata.as_ref())
        {
            let Some(call_id) = extract_tool_call_id(&tool_call).map(str::trim) else {
                continue;
            };
            if call_id.is_empty() {
                continue;
            }
            let Some(name) = extract_tool_call_name(&tool_call).map(str::trim) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            if !has_remaining_tool_output(remaining_tool_output_ids, call_id) {
                continue;
            }
            let arguments = tool_call_arguments_text(&tool_call);
            items.push(build_function_call_item(call_id, name, arguments.as_str()));
            seen_tool_call_ids.insert(call_id.to_string());
        }
        return items;
    }

    if matches!(role, "user" | "system" | "developer") && !record.content.trim().is_empty() {
        items.push(to_message_item(
            role,
            &Value::String(record.content.clone()),
            false,
        ));
    }
    items
}

fn collect_tool_output_id_counts(records: &[EngineRecord]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for record in records {
        if record.role.trim() != "tool" {
            continue;
        }
        if let Some(tool_call_id) = engine_record_tool_call_id(record) {
            *counts.entry(tool_call_id).or_insert(0) += 1;
        }
    }
    counts
}

fn engine_record_tool_call_id(record: &EngineRecord) -> Option<String> {
    record
        .metadata
        .as_ref()
        .and_then(|value| {
            value
                .get("tool_call_id")
                .or_else(|| value.get("toolCallId"))
                .or_else(|| value.get("tool_callId"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn has_remaining_tool_output(counts: &HashMap<String, usize>, call_id: &str) -> bool {
    counts.get(call_id).copied().unwrap_or_default() > 0
}

fn decrement_remaining_tool_output_id(counts: &mut HashMap<String, usize>, call_id: &str) {
    let should_remove = if let Some(count) = counts.get_mut(call_id) {
        *count = count.saturating_sub(1);
        *count == 0
    } else {
        false
    };
    if should_remove {
        counts.remove(call_id);
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

#[derive(Debug, Clone, Default)]
struct RecordBatchLogSummary {
    record_count: usize,
    roles: String,
    record_ids: String,
    tool_names: String,
    content_bytes: usize,
    max_content_bytes: usize,
    metadata_bytes: usize,
    structured_payload_bytes: usize,
}

#[derive(Debug, Clone, Default)]
struct SaveRecordLogSummary {
    role: String,
    conversation_id: String,
    conversation_turn_id: String,
    message_id: String,
    tool_call_id: String,
    content_bytes: usize,
}

#[derive(Debug, Clone, Default)]
struct SaveAssistantRecordLogSummary {
    conversation_id: String,
    conversation_turn_id: String,
    message_id: String,
    response_id: String,
    response_status: String,
    content_bytes: usize,
}

#[derive(Debug, Clone, Default)]
struct SaveToolRecordLogSummary {
    conversation_id: String,
    conversation_turn_id: String,
    message_id: String,
    tool_call_id: String,
    tool_name: String,
    content_bytes: usize,
}

#[derive(Debug, Clone, Default)]
struct SaveToolRecordsLogSummary {
    record_count: usize,
    conversation_ids: String,
    conversation_turn_ids: String,
    tool_call_ids: String,
    tool_names: String,
    content_bytes: usize,
    max_content_bytes: usize,
}

fn summarize_record_batch(records: &[UpsertRecordInput]) -> RecordBatchLogSummary {
    let mut roles = Vec::new();
    let mut record_ids = Vec::new();
    let mut tool_names = Vec::new();
    let mut content_bytes = 0usize;
    let mut max_content_bytes = 0usize;
    let mut metadata_bytes = 0usize;
    let mut structured_payload_bytes = 0usize;

    for record in records {
        roles.push(record.role.as_str());
        record_ids.push(record.id.as_str());
        if let Some(tool_name) = record.metadata.as_ref().and_then(metadata_tool_name) {
            tool_names.push(tool_name);
        }
        let current_content_bytes = record.content.len();
        content_bytes += current_content_bytes;
        max_content_bytes = max_content_bytes.max(current_content_bytes);
        metadata_bytes += optional_json_bytes(&record.metadata);
        structured_payload_bytes += optional_json_bytes(&record.structured_payload);
    }

    RecordBatchLogSummary {
        record_count: records.len(),
        roles: summarize_values(roles),
        record_ids: summarize_values(record_ids),
        tool_names: summarize_values(tool_names),
        content_bytes,
        max_content_bytes,
        metadata_bytes,
        structured_payload_bytes,
    }
}

fn summarize_save_record_input(input: &SaveRecordInput) -> SaveRecordLogSummary {
    SaveRecordLogSummary {
        role: input.role.clone(),
        conversation_id: input.conversation_id.clone(),
        conversation_turn_id: input.conversation_turn_id.clone().unwrap_or_default(),
        message_id: input.message_id.clone().unwrap_or_default(),
        tool_call_id: input.tool_call_id.clone().unwrap_or_default(),
        content_bytes: input.content.len(),
    }
}

fn summarize_assistant_record_input(
    input: &SaveAssistantRecordInput,
) -> SaveAssistantRecordLogSummary {
    SaveAssistantRecordLogSummary {
        conversation_id: input.conversation_id.clone(),
        conversation_turn_id: input.conversation_turn_id.clone().unwrap_or_default(),
        message_id: input.message_id.clone().unwrap_or_default(),
        response_id: input.response_id.clone().unwrap_or_default(),
        response_status: input.response_status.clone().unwrap_or_default(),
        content_bytes: input.content.len(),
    }
}

fn summarize_tool_record_input(input: &SaveToolRecordInput) -> SaveToolRecordLogSummary {
    SaveToolRecordLogSummary {
        conversation_id: input.conversation_id.clone(),
        conversation_turn_id: input.conversation_turn_id.clone().unwrap_or_default(),
        message_id: input.message_id.clone().unwrap_or_default(),
        tool_call_id: input.tool_call_id.clone(),
        tool_name: input.tool_name.clone(),
        content_bytes: input.content.len(),
    }
}

fn summarize_tool_record_inputs(inputs: &[SaveToolRecordInput]) -> SaveToolRecordsLogSummary {
    let mut conversation_ids = Vec::new();
    let mut conversation_turn_ids = Vec::new();
    let mut tool_call_ids = Vec::new();
    let mut tool_names = Vec::new();
    let mut content_bytes = 0usize;
    let mut max_content_bytes = 0usize;

    for input in inputs {
        conversation_ids.push(input.conversation_id.as_str());
        if let Some(turn_id) = input.conversation_turn_id.as_deref() {
            conversation_turn_ids.push(turn_id);
        }
        tool_call_ids.push(input.tool_call_id.as_str());
        tool_names.push(input.tool_name.as_str());
        let current_content_bytes = input.content.len();
        content_bytes += current_content_bytes;
        max_content_bytes = max_content_bytes.max(current_content_bytes);
    }

    SaveToolRecordsLogSummary {
        record_count: inputs.len(),
        conversation_ids: summarize_values(conversation_ids),
        conversation_turn_ids: summarize_values(conversation_turn_ids),
        tool_call_ids: summarize_values(tool_call_ids),
        tool_names: summarize_values(tool_names),
        content_bytes,
        max_content_bytes,
    }
}

fn metadata_tool_name(metadata: &Value) -> Option<&str> {
    metadata
        .get("toolName")
        .or_else(|| metadata.get("tool_name"))
        .or_else(|| metadata.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn optional_json_bytes(value: &Option<Value>) -> usize {
    value
        .as_ref()
        .and_then(|value| serde_json::to_vec(value).ok())
        .map(|bytes| bytes.len())
        .unwrap_or_default()
}

fn summarize_values(values: Vec<&str>) -> String {
    const LIMIT: usize = 8;
    let mut unique = Vec::<&str>::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() || unique.contains(&value) {
            continue;
        }
        unique.push(value);
    }

    let omitted = unique.len().saturating_sub(LIMIT);
    let mut summary = unique.into_iter().take(LIMIT).collect::<Vec<_>>().join(",");
    if omitted > 0 {
        if !summary.is_empty() {
            summary.push(',');
        }
        summary.push_str(format!("+{omitted} more").as_str());
    }
    summary
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use memory_engine_sdk::{
        ComposeContextBlock, ComposeContextMeta, ComposeContextPolicy, ComposeContextResponse,
        EngineRecord,
    };
    use serde_json::json;

    use super::{
        compose_response_to_input_items, MemoryContextComposer, MemoryRecordScope, MemoryScope,
    };

    #[test]
    fn memory_scope_builder_keeps_runtime_source_key() {
        let policy = ComposeContextPolicy {
            include_recent_records: Some(false),
            include_thread_summary: Some(true),
            include_subject_memory: Some(true),
            recent_record_limit: Some(12),
            summary_limit: Some(3),
        };
        let scope = MemoryScope::thread("tenant_1", "task_runner", "task_thread_1")
            .with_subject_id("contact_1")
            .with_related_subject_ids(["project_1", "agent_1"])
            .with_policy(policy);

        assert_eq!(scope.tenant_id, "tenant_1");
        assert_eq!(scope.source_id, "task_runner");
        assert_eq!(scope.thread_id, "task_thread_1");
        assert_eq!(scope.subject_id.as_deref(), Some("contact_1"));
        assert_eq!(scope.related_subject_ids, vec!["project_1", "agent_1"]);
        assert_eq!(
            scope
                .policy
                .as_ref()
                .and_then(|value| value.recent_record_limit),
            Some(12)
        );
    }

    #[test]
    fn memory_record_scope_builder_defaults_to_pending_message_records() {
        let scope = MemoryRecordScope::new("tenant_1")
            .with_thread_id("thread_1")
            .with_record_type("task_message")
            .with_default_summary_status(None);

        assert_eq!(scope.tenant_id, "tenant_1");
        assert_eq!(scope.thread_id.as_deref(), Some("thread_1"));
        assert_eq!(scope.record_type, "task_message");
        assert!(scope.default_summary_status.is_none());

        let message_scope = MemoryRecordScope::message_thread("tenant_1", "thread_2");
        assert_eq!(message_scope.record_type, "message");
        assert_eq!(
            message_scope.default_summary_status.as_deref(),
            Some("pending")
        );
    }

    #[test]
    fn direct_composer_rejects_mismatched_scope_source_key() {
        let composer = MemoryContextComposer::new_direct(
            "http://127.0.0.1:1",
            Duration::from_secs(1),
            "chatos",
        )
        .expect("composer");
        assert_eq!(composer.source_id(), Some("chatos"));

        let matching = MemoryScope::thread("tenant_1", "chatos", "thread_1");
        composer
            .validate_scope_source(&matching)
            .expect("matching scope source");

        let mismatched = MemoryScope::thread("tenant_1", "task_runner", "thread_1");
        let err = composer
            .validate_scope_source(&mismatched)
            .expect_err("mismatched scope source");
        assert!(err.contains("source_id mismatch"));
    }

    #[test]
    fn compose_response_to_input_items_rebuilds_tool_exchange_in_responses_shape() {
        let response = ComposeContextResponse {
            thread_id: "thread-1".to_string(),
            blocks: vec![ComposeContextBlock {
                block_type: "summary".to_string(),
                text: "recent summary".to_string(),
            }],
            recent_records: vec![
                EngineRecord {
                    id: "rec-1".to_string(),
                    thread_id: "thread-1".to_string(),
                    tenant_id: "tenant-1".to_string(),
                    source_id: "task".to_string(),
                    external_record_id: None,
                    role: "assistant".to_string(),
                    record_type: "message".to_string(),
                    content: "calling tool".to_string(),
                    structured_payload: None,
                    metadata: Some(json!({
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "demo.search",
                                "arguments": "{\"q\":\"rust\"}"
                            }
                        }]
                    })),
                    summary_status: "pending".to_string(),
                    summary_id: None,
                    summarized_at: None,
                    created_at: "2026-06-08T00:00:00Z".to_string(),
                },
                EngineRecord {
                    id: "rec-2".to_string(),
                    thread_id: "thread-1".to_string(),
                    tenant_id: "tenant-1".to_string(),
                    source_id: "task".to_string(),
                    external_record_id: None,
                    role: "tool".to_string(),
                    record_type: "message".to_string(),
                    content: "done".to_string(),
                    structured_payload: None,
                    metadata: Some(json!({
                        "tool_call_id": "call_1"
                    })),
                    summary_status: "pending".to_string(),
                    summary_id: None,
                    summarized_at: None,
                    created_at: "2026-06-08T00:00:01Z".to_string(),
                },
            ],
            meta: ComposeContextMeta {
                summary_count: 1,
                recent_record_count: 2,
            },
        };

        let items = compose_response_to_input_items(&response);
        assert_eq!(
            items[0].get("type").and_then(|value| value.as_str()),
            Some("message")
        );
        assert!(items.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
        }));
        assert!(items.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
        }));
        assert!(!items
            .iter()
            .any(|item| { item.get("role").and_then(|value| value.as_str()) == Some("tool") }));
    }

    #[test]
    fn compose_response_to_input_items_skips_orphan_tool_outputs() {
        let response = ComposeContextResponse {
            thread_id: "thread-1".to_string(),
            blocks: Vec::new(),
            recent_records: vec![EngineRecord {
                id: "rec-1".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "tool".to_string(),
                record_type: "message".to_string(),
                content: "done".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_call_id": "call_missing"
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:01Z".to_string(),
            }],
            meta: ComposeContextMeta {
                summary_count: 0,
                recent_record_count: 1,
            },
        };

        let items = compose_response_to_input_items(&response);
        assert!(items.is_empty());
    }

    #[test]
    fn compose_response_to_input_items_skips_orphan_tool_calls() {
        let response = ComposeContextResponse {
            thread_id: "thread-1".to_string(),
            blocks: Vec::new(),
            recent_records: vec![EngineRecord {
                id: "rec-1".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "assistant".to_string(),
                record_type: "message".to_string(),
                content: "calling tool".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "demo.search",
                            "arguments": "{\"q\":\"rust\"}"
                        }
                    }]
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:00Z".to_string(),
            }],
            meta: ComposeContextMeta {
                summary_count: 0,
                recent_record_count: 1,
            },
        };

        let items = compose_response_to_input_items(&response);

        assert!(items.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("message")
                && item.get("role").and_then(|value| value.as_str()) == Some("assistant")
        }));
        assert!(!items.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call")
        }));
    }

    #[test]
    fn compose_response_to_input_items_omits_large_tool_outputs() {
        let response = ComposeContextResponse {
            thread_id: "thread-1".to_string(),
            blocks: Vec::new(),
            recent_records: vec![
                EngineRecord {
                    id: "rec-1".to_string(),
                    thread_id: "thread-1".to_string(),
                    tenant_id: "tenant-1".to_string(),
                    source_id: "task".to_string(),
                    external_record_id: None,
                    role: "assistant".to_string(),
                    record_type: "message".to_string(),
                    content: "calling tool".to_string(),
                    structured_payload: None,
                    metadata: Some(json!({
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "code.read_file",
                                "arguments": "{\"path\":\"big.log\"}"
                            }
                        }]
                    })),
                    summary_status: "pending".to_string(),
                    summary_id: None,
                    summarized_at: None,
                    created_at: "2026-06-08T00:00:00Z".to_string(),
                },
                EngineRecord {
                    id: "rec-2".to_string(),
                    thread_id: "thread-1".to_string(),
                    tenant_id: "tenant-1".to_string(),
                    source_id: "task".to_string(),
                    external_record_id: None,
                    role: "tool".to_string(),
                    record_type: "message".to_string(),
                    content: "x".repeat(9_000),
                    structured_payload: None,
                    metadata: Some(json!({
                        "tool_call_id": "call_1",
                        "name": "code.read_file"
                    })),
                    summary_status: "pending".to_string(),
                    summary_id: None,
                    summarized_at: None,
                    created_at: "2026-06-08T00:00:01Z".to_string(),
                },
            ],
            meta: ComposeContextMeta {
                summary_count: 0,
                recent_record_count: 2,
            },
        };

        let items = compose_response_to_input_items(&response);
        let output = items
            .iter()
            .find(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
            })
            .and_then(|item| item.get("output"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();

        assert!(output.contains("Tool result omitted"));
        assert!(output.contains("code.read_file"));
        assert!(output.len() < 1_000);
    }

    #[test]
    fn compose_response_to_input_items_reads_tool_calls_from_structured_payload() {
        let response = ComposeContextResponse {
            thread_id: "thread-1".to_string(),
            blocks: Vec::new(),
            recent_records: vec![
                EngineRecord {
                    id: "rec-1".to_string(),
                    thread_id: "thread-1".to_string(),
                    tenant_id: "tenant-1".to_string(),
                    source_id: "task".to_string(),
                    external_record_id: None,
                    role: "assistant".to_string(),
                    record_type: "message".to_string(),
                    content: "calling tool".to_string(),
                    structured_payload: Some(json!([{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "demo.search",
                            "arguments": "{\"q\":\"rust\"}"
                        }
                    }])),
                    metadata: None,
                    summary_status: "pending".to_string(),
                    summary_id: None,
                    summarized_at: None,
                    created_at: "2026-06-08T00:00:00Z".to_string(),
                },
                EngineRecord {
                    id: "rec-2".to_string(),
                    thread_id: "thread-1".to_string(),
                    tenant_id: "tenant-1".to_string(),
                    source_id: "task".to_string(),
                    external_record_id: None,
                    role: "tool".to_string(),
                    record_type: "message".to_string(),
                    content: "done".to_string(),
                    structured_payload: None,
                    metadata: Some(json!({
                        "tool_call_id": "call_1"
                    })),
                    summary_status: "pending".to_string(),
                    summary_id: None,
                    summarized_at: None,
                    created_at: "2026-06-08T00:00:01Z".to_string(),
                },
            ],
            meta: ComposeContextMeta {
                summary_count: 0,
                recent_record_count: 2,
            },
        };

        let items = compose_response_to_input_items(&response);
        assert!(items.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
        }));
        assert!(items.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
        }));
    }
}
