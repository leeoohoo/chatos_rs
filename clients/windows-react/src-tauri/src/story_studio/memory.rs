use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use memory_engine_sdk::{
    ComposeContextPolicy, ComposeContextResponse, EngineRecord, MemoryEngineClient,
    SdkBatchSyncRecordsRequest, SdkComposeContextRequest, SdkUpsertThreadRequest,
    UpsertRecordInput,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::authentication::{runtime_api_base_url, AuthenticationManager};

use super::planning::{
    StoryAgentMemoryCheckpoint, StoryAgentMemoryScope, StoryAgentMessage, StoryAgentMessageRole,
    StoryAgentToolCall, StoryPlanningRun,
};

const MEMORY_SOURCE_ID: &str = "chatos";
const MAXIMUM_MEMORY_RECORDS: usize = 64;
const MAXIMUM_MEMORY_BLOCKS: usize = 16;
const MAXIMUM_MEMORY_CONTEXT_BYTES: usize = 2 * 1_024 * 1_024;

pub(super) struct StoryMemoryAdapter {
    client: MemoryEngineClient,
}

impl StoryMemoryAdapter {
    pub(super) async fn for_owner(
        authentication: &AuthenticationManager,
        owner_user_id: &str,
    ) -> Result<Self, String> {
        let token = authentication.access_token_for_owner(owner_user_id).await?;
        let client = MemoryEngineClient::new_direct(
            memory_base_url()?,
            Duration::from_secs(30),
            MEMORY_SOURCE_ID,
        )?
        .with_bearer_token(token);
        Ok(Self { client })
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
        records: Vec<UpsertRecordInput>,
    ) -> Result<(), String> {
        validate_scope(scope, owner_user_id)?;
        if records.is_empty() || records.len() > 32 {
            return Err(memory_contract_error());
        }
        let expected_count = records.len();
        let response = self
            .client
            .batch_sync_records(
                &scope.thread_id,
                &SdkBatchSyncRecordsRequest {
                    tenant_id: owner_user_id.to_owned(),
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
        for expected in records {
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
    ) -> Result<ComposeContextResponse, String> {
        validate_scope(scope, owner_user_id)?;
        let response = self
            .client
            .compose_context(&SdkComposeContextRequest {
                tenant_id: owner_user_id.to_owned(),
                subject_id: Some(scope.subject_id.clone()),
                related_subject_ids: None,
                thread_id: scope.thread_id.clone(),
                policy: Some(ComposeContextPolicy {
                    include_recent_records: Some(true),
                    include_thread_summary: Some(true),
                    include_subject_memory: Some(true),
                    recent_record_limit: Some(MAXIMUM_MEMORY_RECORDS),
                    summary_limit: Some(2),
                }),
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
        thread_id: format!(
            "client-agent:story:{}:{}",
            run.project_id, run.checkpoint.id
        ),
        subject_id: format!("client-agent:story:{}", run.project_id),
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
                &run.checkpoint.messages[index],
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
        let message = run
            .checkpoint
            .messages
            .get(index)
            .ok_or_else(memory_contract_error)?;
        let expected = memory_record(&memory.scope, memory.record_epoch_unix_ms, index, message)?;
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
        || memory.scope.subject_id != format!("client-agent:story:{}", run.project_id)
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
    message: &StoryAgentMessage,
) -> Result<UpsertRecordInput, String> {
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
    ]);
    if let Some(call_id) = &message.tool_call_id {
        metadata.insert("tool_call_id".into(), Value::String(call_id.clone()));
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

fn memory_transport_error(_error: String) -> String {
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
