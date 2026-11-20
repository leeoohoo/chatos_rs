use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use memory_engine_sdk::{
    ComposeContextPolicy, ComposeContextResponse, EngineRecord, MemoryEngineClient,
    SdkBatchSyncRecordsRequest, SdkComposeContextRequest, SdkUpsertThreadRequest,
    UpsertRecordInput,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::authentication::{runtime_api_base_url, AuthenticationManager};

use super::planning::{
    StoryAgentMemoryCheckpoint, StoryAgentMemoryScope, StoryAgentMessage, StoryAgentMessageRole,
    StoryAgentToolCall, StoryModelCapabilitySnapshot, StoryPlanningRun,
};

const MEMORY_SOURCE_ID: &str = "chatos";
const MAXIMUM_MEMORY_RECORDS: usize = 64;
const MAXIMUM_MEMORY_BLOCKS: usize = 16;
const MAXIMUM_MEMORY_CONTEXT_BYTES: usize = 2 * 1_024 * 1_024;

pub(super) struct StoryMemoryAdapter {
    client: MemoryEngineClient,
    assertion_http: reqwest::Client,
    assertion_base_url: String,
    access_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct IssueRuntimeContextAssertionRequest<'a> {
    runtime_kind: &'a str,
    project_id: Option<&'a str>,
    runtime_id: &'a str,
    operation: &'a str,
    source_id: Option<&'a str>,
    canonical_agent_subject_id: Option<&'a str>,
    thread_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct IssueRuntimeContextAssertionResponse {
    runtime_context_assertion: String,
    source_id: String,
    canonical_agent_subject_id: String,
    thread_id: String,
    project_partition_key: String,
    runtime_identity_revision: String,
}

impl StoryMemoryAdapter {
    pub(super) async fn for_owner(
        authentication: &AuthenticationManager,
        owner_user_id: &str,
    ) -> Result<Self, String> {
        let token = authentication.access_token_for_owner(owner_user_id).await?;
        let assertion_http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| memory_configuration_error())?;
        let client = MemoryEngineClient::new_direct(
            memory_base_url()?,
            Duration::from_secs(30),
            MEMORY_SOURCE_ID,
        )?
        .with_bearer_token(token.clone());
        Ok(Self {
            client,
            assertion_http,
            assertion_base_url: runtime_api_base_url().trim_end_matches('/').to_string(),
            access_token: token,
        })
    }

    pub(super) async fn ensure_thread(
        &self,
        owner_user_id: &str,
        scope: &StoryAgentMemoryScope,
    ) -> Result<(), String> {
        validate_scope(scope, owner_user_id)?;
        let thread = self
            .client
            .upsert_thread(
                &scope.thread_id,
                &SdkUpsertThreadRequest {
                    tenant_id: owner_user_id.to_owned(),
                    subject_id: scope.subject_id.clone(),
                    thread_type: "client_agent".into(),
                    external_thread_id: Some(scope.thread_id.clone()),
                    title: None,
                    labels: Some(vec![
                        "client_agent".into(),
                        "memory_mapping:client_agent.v1".into(),
                    ]),
                    metadata: None,
                    status: None,
                    created_at: None,
                    updated_at: None,
                    archived_at: None,
                },
            )
            .await
            .map_err(memory_transport_error)?;
        if thread.id != scope.thread_id
            || thread.tenant_id != owner_user_id
            || thread.source_id != scope.source_id
            || thread.subject_id != scope.subject_id
        {
            return Err(memory_contract_error());
        }
        Ok(())
    }

    pub(super) async fn sync_records(
        &self,
        owner_user_id: &str,
        scope: &StoryAgentMemoryScope,
        mut records: Vec<UpsertRecordInput>,
    ) -> Result<(), String> {
        validate_scope(scope, owner_user_id)?;
        if records.is_empty() || records.len() > 32 {
            return Err(memory_contract_error());
        }
        let expected_count = records.len();
        let assertion = self
            .issue_runtime_context_assertion(owner_user_id, scope, "record_ingest")
            .await?;
        enrich_source_unit_records(&mut records, assertion.runtime_identity_revision.as_str())?;
        let response = self
            .client
            .ingest_thread_records(
                &scope.thread_id,
                &SdkBatchSyncRecordsRequest {
                    tenant_id: owner_user_id.to_owned(),
                    project_partition_key: assertion.project_partition_key,
                    runtime_context_assertion: assertion.runtime_context_assertion,
                    records,
                },
            )
            .await
            .map_err(memory_transport_error)?;
        if response.thread_id != scope.thread_id
            || response.received_count != expected_count
            || response.upserted_count != expected_count
        {
            return Err(memory_contract_error());
        }
        Ok(())
    }

    pub(super) async fn reconcile_records(
        &self,
        owner_user_id: &str,
        scope: &StoryAgentMemoryScope,
        records: &[UpsertRecordInput],
    ) -> Result<(), String> {
        validate_scope(scope, owner_user_id)?;
        if records.is_empty() || records.len() > 32 {
            return Err(memory_contract_error());
        }
        let assertion = self
            .issue_runtime_context_assertion(owner_user_id, scope, "record_ingest")
            .await?;
        let mut expected_records = records.to_vec();
        enrich_source_unit_records(
            expected_records.as_mut_slice(),
            assertion.runtime_identity_revision.as_str(),
        )?;
        for expected in &expected_records {
            let stored = self
                .client
                .get_record(&expected.id, Some(owner_user_id), Some(&scope.thread_id))
                .await
                .map_err(memory_transport_error)?
                .ok_or_else(memory_sync_uncertain)?;
            if !record_matches(&stored, expected, owner_user_id, scope) {
                return Err(memory_contract_error());
            }
        }
        Ok(())
    }

    pub(super) async fn compose(
        &self,
        owner_user_id: &str,
        scope: &StoryAgentMemoryScope,
        capability: &StoryModelCapabilitySnapshot,
        query_text: &str,
        current_turn_id: &str,
    ) -> Result<ComposeContextResponse, String> {
        validate_scope(scope, owner_user_id)?;
        if capability.revision.trim().is_empty()
            || capability.protocol_overhead_estimator_id.trim().is_empty()
            || capability.context_window_tokens < 2_048
            || capability.output_reserve_tokens < 256
            || capability.output_reserve_tokens >= capability.context_window_tokens
            || query_text.trim().is_empty()
            || current_turn_id.trim().is_empty()
        {
            return Err(memory_contract_error());
        }
        let assertion = self
            .issue_runtime_context_assertion(owner_user_id, scope, "compose")
            .await?;
        let response = self
            .client
            .compose_context(&SdkComposeContextRequest {
                tenant_id: owner_user_id.to_owned(),
                subject_id: Some(scope.subject_id.clone()),
                context_project_subject_id: Some(format!("project:{}", scope.project_id)),
                thread_id: scope.thread_id.clone(),
                query_text: Some(query_text.to_string()),
                current_turn_id: Some(current_turn_id.to_string()),
                model_capability_revision: capability.revision.clone(),
                policy: ComposeContextPolicy {
                    max_memory_tokens: 24_000,
                    max_recent_record_tokens: 12_000,
                    max_thread_summary_tokens: 6_000,
                    max_project_memory_tokens: 4_000,
                    max_agent_memory_tokens: 4_000,
                },
                runtime_context_assertion: assertion.runtime_context_assertion,
            })
            .await
            .map_err(memory_transport_error)?;
        if response.thread_id != scope.thread_id
            || response.meta.recent_record_count != response.recent_records.len()
            || response.recent_records.len() > MAXIMUM_MEMORY_RECORDS
            || response.blocks.len() > MAXIMUM_MEMORY_BLOCKS
            || serde_json::to_vec(&response)
                .map(|bytes| bytes.len() > MAXIMUM_MEMORY_CONTEXT_BYTES)
                .unwrap_or(true)
        {
            return Err(memory_contract_error());
        }
        Ok(response)
    }

    async fn issue_runtime_context_assertion(
        &self,
        owner_user_id: &str,
        scope: &StoryAgentMemoryScope,
        operation: &str,
    ) -> Result<IssueRuntimeContextAssertionResponse, String> {
        validate_scope(scope, owner_user_id)?;
        let response = self
            .assertion_http
            .post(format!(
                "{}/api/local-connectors/runtime-context-assertions",
                self.assertion_base_url
            ))
            .bearer_auth(self.access_token.as_str())
            .json(&IssueRuntimeContextAssertionRequest {
                runtime_kind: "story_studio",
                project_id: Some(scope.project_id.as_str()),
                runtime_id: scope.run_id.as_str(),
                operation,
                source_id: None,
                canonical_agent_subject_id: None,
                thread_id: None,
            })
            .send()
            .await
            .map_err(memory_transport_error)?;
        if !response.status().is_success() {
            return Err(memory_transport_error(format!(
                "runtime context assertion issuer returned {}",
                response.status()
            )));
        }
        let assertion = response
            .json::<IssueRuntimeContextAssertionResponse>()
            .await
            .map_err(memory_transport_error)?;
        if assertion.runtime_context_assertion.trim().is_empty()
            || assertion.source_id != scope.source_id
            || assertion.canonical_agent_subject_id != scope.subject_id
            || assertion.thread_id != scope.thread_id
            || assertion.project_partition_key != format!("project:{}", scope.project_id)
            || assertion.runtime_identity_revision.trim().is_empty()
        {
            return Err(memory_contract_error());
        }
        Ok(assertion)
    }
}

pub(super) fn bind_memory(
    run: &mut StoryPlanningRun,
    owner_user_id: &str,
    now_unix_ms: u64,
) -> Result<(), String> {
    if let Some(memory) = run.checkpoint.memory.as_ref() {
        validate_checkpoint(run, owner_user_id, memory)?;
        run.uses_cloud_memory = true;
        return Ok(());
    }
    if run.checkpoint.model_calls != 0
        || !run.checkpoint.pending_calls.is_empty()
        || run.checkpoint.in_flight_call_id.is_some()
        || run.checkpoint.messages.is_empty()
        || run.checkpoint.messages.iter().any(|message| {
            !matches!(
                message.role,
                StoryAgentMessageRole::System | StoryAgentMessageRole::User
            ) || !message.tool_calls.is_empty()
                || message.tool_call_id.is_some()
        })
    {
        return Err(memory_contract_error());
    }
    let scope = StoryAgentMemoryScope {
        tenant_hash: run.owner_hash.clone(),
        source_id: MEMORY_SOURCE_ID.into(),
        project_id: run.project_id.clone(),
        thread_id: format!(
            "client-agent:story:{}:{}",
            run.project_id, run.checkpoint.id
        ),
        subject_id: format!("agent:story-studio:{}", run.project_id),
        project_subject_id: format!("project:{}", run.project_id),
        run_id: run.checkpoint.id.clone(),
        runtime_scope: run.checkpoint.scope.clone(),
    };
    run.checkpoint.memory = Some(StoryAgentMemoryCheckpoint {
        scope,
        record_epoch_unix_ms: now_unix_ms,
        pinned_message_count: run.checkpoint.messages.len(),
        synced_message_count: 0,
        synced_digest: String::new(),
        thread_created: false,
        sync_in_flight_end: None,
    });
    run.uses_cloud_memory = true;
    validate_memory(run, owner_user_id)
}

pub(super) fn validate_memory(run: &StoryPlanningRun, owner_user_id: &str) -> Result<(), String> {
    match run.checkpoint.memory.as_ref() {
        Some(memory) if run.uses_cloud_memory => validate_checkpoint(run, owner_user_id, memory),
        None if !run.uses_cloud_memory => Ok(()),
        _ => Err(memory_contract_error()),
    }
}

pub(super) fn memory_records(
    run: &StoryPlanningRun,
    start: usize,
    end: usize,
) -> Result<Vec<UpsertRecordInput>, String> {
    let memory = run
        .checkpoint
        .memory
        .as_ref()
        .ok_or_else(memory_contract_error)?;
    if start >= end || end > run.checkpoint.messages.len() || end - start > 32 {
        return Err(memory_contract_error());
    }
    (start..end)
        .map(|index| {
            memory_record(
                &memory.scope,
                memory.record_epoch_unix_ms,
                index,
                run.checkpoint.messages.as_slice(),
            )
        })
        .collect()
}

pub(super) fn synced_digest(run: &StoryPlanningRun, end: usize) -> Result<String, String> {
    if end > run.checkpoint.messages.len() {
        return Err(memory_contract_error());
    }
    let bytes =
        serde_json::to_vec(&run.checkpoint.messages[..end]).map_err(|_| memory_contract_error())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(super) fn assemble_memory_input(
    run: &StoryPlanningRun,
    response: &ComposeContextResponse,
) -> Result<Vec<Value>, String> {
    let memory = run
        .checkpoint
        .memory
        .as_ref()
        .ok_or_else(memory_contract_error)?;
    validate_checkpoint(run, &run.owner_hash, memory)?;
    if response.thread_id != memory.scope.thread_id
        || response.meta.recent_record_count != response.recent_records.len()
        || response.recent_records.len() > MAXIMUM_MEMORY_RECORDS
        || response.blocks.len() > MAXIMUM_MEMORY_BLOCKS
    {
        return Err(memory_contract_error());
    }
    let mut selected = HashSet::new();
    let mut selected_records = HashSet::new();
    let mut retained_indices = Vec::new();
    for record in &response.recent_records {
        if !selected_records.insert(record.id.clone()) {
            return Err(memory_contract_error());
        }
        let index = record_index(&record.id, &memory.scope.run_id)?;
        if index >= memory.synced_message_count {
            return Err(memory_contract_error());
        }
        let expected = memory_record(
            &memory.scope,
            memory.record_epoch_unix_ms,
            index,
            run.checkpoint.messages.as_slice(),
        )?;
        if !record_matches_hashed_tenant(record, &expected, &memory.scope)
            || !selected.insert(index)
        {
            return Err(memory_contract_error());
        }
        if index >= memory.pinned_message_count {
            retained_indices.push(index);
        }
    }
    if response
        .blocks
        .iter()
        .all(|block| block.text.trim().is_empty())
        && !(memory.pinned_message_count..run.checkpoint.messages.len())
            .all(|index| selected.contains(&index))
    {
        return Err(memory_contract_error());
    }

    let retained = paired_messages(
        retained_indices
            .into_iter()
            .map(|index| run.checkpoint.messages[index].clone())
            .collect(),
    );
    let mut input = Vec::new();
    let block_text = response
        .blocks
        .iter()
        .map(|block| format!("[{}]\n{}", block.block_type, block.text))
        .collect::<Vec<_>>()
        .join("\n\n===\n\n");
    if !block_text.trim().is_empty() {
        input.push(json!({"type": "message", "role": "system", "content": block_text}));
    }
    input.extend(super::runtime::checkpoint_input(&retained)?);
    let pinned_users = run.checkpoint.messages[..memory.pinned_message_count]
        .iter()
        .filter(|message| message.role == StoryAgentMessageRole::User)
        .cloned()
        .collect::<Vec<_>>();
    input.extend(super::runtime::checkpoint_input(&pinned_users)?);
    Ok(input)
}

fn validate_checkpoint(
    run: &StoryPlanningRun,
    owner_user_id: &str,
    memory: &StoryAgentMemoryCheckpoint,
) -> Result<(), String> {
    validate_scope(&memory.scope, owner_user_id)?;
    if memory.scope.run_id != run.checkpoint.id
        || memory.scope.runtime_scope != run.checkpoint.scope
        || memory.scope.thread_id
            != format!(
                "client-agent:story:{}:{}",
                run.project_id, run.checkpoint.id
            )
        || memory.scope.subject_id != format!("agent:story-studio:{}", run.project_id)
        || memory.pinned_message_count == 0
        || memory.pinned_message_count > run.checkpoint.messages.len()
        || memory.synced_message_count > run.checkpoint.messages.len()
        || memory.sync_in_flight_end.is_some_and(|end| {
            end <= memory.synced_message_count
                || end > run.checkpoint.messages.len()
                || end - memory.synced_message_count > 32
        })
        || run.checkpoint.messages[..memory.pinned_message_count]
            .iter()
            .any(|message| {
                !matches!(
                    message.role,
                    StoryAgentMessageRole::System | StoryAgentMessageRole::User
                ) || !message.tool_calls.is_empty()
                    || message.tool_call_id.is_some()
            })
    {
        return Err(memory_contract_error());
    }
    if memory.synced_message_count == 0 {
        if !memory.synced_digest.is_empty() {
            return Err(memory_contract_error());
        }
    } else if synced_digest(run, memory.synced_message_count)? != memory.synced_digest {
        return Err(memory_contract_error());
    }
    Ok(())
}

fn validate_scope(scope: &StoryAgentMemoryScope, owner_identity: &str) -> Result<(), String> {
    let owner_hash = if owner_identity.len() == 64
        && owner_identity
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        owner_identity.to_ascii_lowercase()
    } else {
        let owner = owner_identity.trim();
        if owner.is_empty() || owner.len() > 1_024 {
            return Err(memory_contract_error());
        }
        format!("{:x}", Sha256::digest(owner.as_bytes()))
    };
    if scope.tenant_hash != owner_hash
        || scope.source_id != MEMORY_SOURCE_ID
        || scope.project_id.trim().is_empty()
        || scope.project_id.chars().any(char::is_control)
        || scope.thread_id != format!("client-agent:story:{}:{}", scope.project_id, scope.run_id)
        || scope.subject_id != format!("agent:story-studio:{}", scope.project_id)
        || scope.thread_id.trim().is_empty()
        || scope.subject_id.trim().is_empty()
        || scope.run_id.trim().is_empty()
        || scope.runtime_scope.trim().is_empty()
    {
        return Err(memory_contract_error());
    }
    Ok(())
}

fn memory_record(
    scope: &StoryAgentMemoryScope,
    record_epoch_unix_ms: u64,
    index: usize,
    messages: &[StoryAgentMessage],
) -> Result<UpsertRecordInput, String> {
    let message = messages.get(index).ok_or_else(memory_contract_error)?;
    let mut payload = Map::new();
    if !message.tool_calls.is_empty() {
        payload.insert(
            "tool_calls".into(),
            Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {"name": call.name, "arguments": call.arguments}
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Some(call_id) = &message.tool_call_id {
        payload.insert("tool_call_id".into(), Value::String(call_id.clone()));
    }
    let mut metadata = Map::from_iter([
        (
            "client_agent_run_id".into(),
            Value::String(scope.run_id.clone()),
        ),
        (
            "client_agent_message_index".into(),
            Value::from(index as u64),
        ),
        (
            "conversation_turn_id".into(),
            Value::String(conversation_turn_id(scope, messages, index)?),
        ),
    ]);
    if let Some(call_id) = &message.tool_call_id {
        metadata.insert("tool_call_id".into(), Value::String(call_id.clone()));
        let (assistant_index, assistant) = messages[..index]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, candidate)| {
                candidate.role == StoryAgentMessageRole::Assistant
                    && candidate.tool_calls.iter().any(|call| &call.id == call_id)
            })
            .ok_or_else(memory_contract_error)?;
        let ordinal = assistant
            .tool_calls
            .iter()
            .position(|call| &call.id == call_id)
            .ok_or_else(memory_contract_error)?;
        metadata.insert(
            "assistant_record_id".into(),
            Value::String(format!(
                "client-agent:{}:message:{assistant_index}",
                scope.run_id
            )),
        );
        metadata.insert(
            "tool_exchange_id".into(),
            Value::String(format!("story:{}:tool:{assistant_index}", scope.run_id)),
        );
        metadata.insert("tool_result_ordinal".into(), Value::from(ordinal as u64));
        metadata.insert(
            "tool_result_count".into(),
            Value::from(assistant.tool_calls.len() as u64),
        );
    }
    if let Some(output) = &message.response_output_json {
        metadata.insert(
            "responses_output".into(),
            serde_json::from_slice(output).map_err(|_| memory_contract_error())?,
        );
    }
    if let Some(usage) = &message.usage {
        metadata.insert(
            "provider_usage".into(),
            json!({
                "input_tokens": usage.input_tokens,
                "cached_tokens": usage.cached_tokens,
                "output_tokens": usage.output_tokens,
                "requests": usage.requests,
            }),
        );
    }
    let index_ms = u64::try_from(index).map_err(|_| memory_contract_error())?;
    let timestamp = record_epoch_unix_ms
        .checked_add(index_ms)
        .and_then(|value| i64::try_from(value).ok())
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .ok_or_else(memory_contract_error)?;
    Ok(UpsertRecordInput {
        id: format!("client-agent:{}:message:{index}", scope.run_id),
        external_record_id: None,
        role: match message.role {
            StoryAgentMessageRole::System => "system",
            StoryAgentMessageRole::User => "user",
            StoryAgentMessageRole::Assistant => "assistant",
            StoryAgentMessageRole::Tool => "tool",
        }
        .into(),
        record_type: "message".into(),
        content: message.content.clone(),
        structured_payload: (!payload.is_empty()).then_some(Value::Object(payload)),
        metadata: Some(Value::Object(metadata)),
        summary_status: None,
        summary_id: None,
        summarized_at: None,
        created_at: timestamp.to_rfc3339_opts(SecondsFormat::Millis, true),
    })
}

fn enrich_source_unit_records(
    records: &mut [UpsertRecordInput],
    runtime_identity_revision: &str,
) -> Result<(), String> {
    if runtime_identity_revision.trim().is_empty() {
        return Err(memory_contract_error());
    }
    for record in records {
        let digest_input = json!({
            "id": record.id,
            "role": record.role,
            "record_type": record.record_type,
            "content": record.content,
            "structured_payload": record.structured_payload,
        });
        let digest = serde_json::to_vec(&digest_input).map_err(|_| memory_contract_error())?;
        let metadata = record
            .metadata
            .get_or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(memory_contract_error)?;
        metadata.insert(
            "project_context_assertion_id".into(),
            Value::String(runtime_identity_revision.to_string()),
        );
        metadata.insert(
            "content_identity_digest".into(),
            Value::String(format!("sha256:{:x}", Sha256::digest(digest))),
        );
    }
    Ok(())
}

pub(super) fn current_memory_query(run: &StoryPlanningRun) -> Result<(String, String), String> {
    let memory = run
        .checkpoint
        .memory
        .as_ref()
        .ok_or_else(memory_contract_error)?;
    let (index, message) = run
        .checkpoint
        .messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, message)| {
            message.role == StoryAgentMessageRole::User && !message.content.trim().is_empty()
        })
        .ok_or_else(memory_contract_error)?;
    Ok((
        message.content.clone(),
        conversation_turn_id(&memory.scope, run.checkpoint.messages.as_slice(), index)?,
    ))
}

fn conversation_turn_id(
    scope: &StoryAgentMemoryScope,
    messages: &[StoryAgentMessage],
    index: usize,
) -> Result<String, String> {
    if index >= messages.len() {
        return Err(memory_contract_error());
    }
    let turn_start = messages[..=index]
        .iter()
        .rposition(|message| message.role == StoryAgentMessageRole::User)
        .unwrap_or(index);
    Ok(format!("story:{}:turn:{turn_start}", scope.run_id))
}

fn record_matches(
    stored: &EngineRecord,
    expected: &UpsertRecordInput,
    owner_user_id: &str,
    scope: &StoryAgentMemoryScope,
) -> bool {
    stored.tenant_id == owner_user_id
        && stored.source_id == scope.source_id
        && record_matches_hashed_tenant(stored, expected, scope)
}

fn record_matches_hashed_tenant(
    stored: &EngineRecord,
    expected: &UpsertRecordInput,
    scope: &StoryAgentMemoryScope,
) -> bool {
    format!("{:x}", Sha256::digest(stored.tenant_id.trim().as_bytes())) == scope.tenant_hash
        && stored.id == expected.id
        && stored.thread_id == scope.thread_id
        && stored.source_id == scope.source_id
        && stored.role == expected.role
        && stored.record_type == expected.record_type
        && stored.content == expected.content
        && stored.structured_payload == expected.structured_payload
        && stored.metadata == expected.metadata
        && stored.created_at == expected.created_at
}

fn record_index(record_id: &str, run_id: &str) -> Result<usize, String> {
    let prefix = format!("client-agent:{run_id}:message:");
    let value = record_id
        .strip_prefix(&prefix)
        .ok_or_else(memory_contract_error)?;
    let index = value
        .parse::<usize>()
        .map_err(|_| memory_contract_error())?;
    if index.to_string() != value {
        return Err(memory_contract_error());
    }
    Ok(index)
}

fn paired_messages(messages: Vec<StoryAgentMessage>) -> Vec<StoryAgentMessage> {
    let mut remaining_outputs = HashMap::<String, usize>::new();
    for message in &messages {
        if message.role == StoryAgentMessageRole::Tool {
            if let Some(call_id) = &message.tool_call_id {
                *remaining_outputs.entry(call_id.clone()).or_default() += 1;
            }
        }
    }
    let mut seen_calls = HashSet::new();
    let mut retained = Vec::new();
    for mut message in messages {
        match message.role {
            StoryAgentMessageRole::Tool => {
                if let Some(call_id) = message.tool_call_id.as_ref() {
                    if seen_calls.contains(call_id) {
                        retained.push(message.clone());
                    }
                    if let Some(count) = remaining_outputs.get_mut(call_id) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            remaining_outputs.remove(call_id);
                        }
                    }
                }
            }
            StoryAgentMessageRole::Assistant => {
                let calls = message
                    .tool_calls
                    .iter()
                    .filter(|call| remaining_outputs.get(&call.id).copied().unwrap_or_default() > 0)
                    .cloned()
                    .collect::<Vec<StoryAgentToolCall>>();
                seen_calls.extend(calls.iter().map(|call| call.id.clone()));
                if !message.content.trim().is_empty() || !calls.is_empty() {
                    message.tool_calls = calls;
                    message.response_output_json = None;
                    retained.push(message);
                }
            }
            StoryAgentMessageRole::System | StoryAgentMessageRole::User => {
                if !message.content.trim().is_empty() {
                    retained.push(message);
                }
            }
        }
    }
    retained
}

fn memory_base_url() -> Result<String, String> {
    let configured = std::env::var("CHATOS_MEMORY_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(runtime_api_base_url);
    let mut url = reqwest::Url::parse(&configured).map_err(|_| memory_configuration_error())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(memory_configuration_error());
    }
    if std::env::var("CHATOS_MEMORY_BASE_URL")
        .ok()
        .is_none_or(|value| value.trim().is_empty())
    {
        let prefix = url.path().trim_matches('/').to_owned();
        let path = if prefix.is_empty() {
            "/api/memory".to_owned()
        } else {
            format!("/{prefix}/api/memory")
        };
        url.set_path(&path);
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn memory_transport_error(_error: impl std::fmt::Display) -> String {
    "剧情规划历史服务暂时不可用，检查点已保留，可稍后恢复。".into()
}

fn memory_sync_uncertain() -> String {
    "剧情规划历史同步结果不明确，需要稍后恢复并对账。".into()
}

fn memory_configuration_error() -> String {
    "剧情规划历史服务地址无效。".into()
}

fn memory_contract_error() -> String {
    "剧情规划历史记录与当前运行不匹配。".into()
}
