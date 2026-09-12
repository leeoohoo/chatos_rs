// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use chatos_agent_profiles::{
    MainChatCapabilitySnapshot, MainChatContextProvider, MainChatProjectSnapshot,
    MainChatPromptSnapshot, MainChatStepContext,
};
use chatos_client_storage::{
    AgentMessageStateRecord, ClientStorage, ListQuery, MediaStateRecord, RecordScope, StorageError,
    StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessageRole, ContextStrategy, FrozenSnapshot, LocalAgentRun,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const MAX_RESOLVED_ATTACHMENT_BYTES: u64 = 5 * 1024 * 1024;
const MAX_TOTAL_RESOLVED_ATTACHMENT_BYTES: u64 = 6 * 1024 * 1024;
const MAXIMUM_SUMMARY_ATTEMPTS: u8 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAttachmentLocator {
    pub attachment_id: String,
    pub media_type: String,
    pub payload_reference: String,
    pub payload_digest: String,
    pub byte_size: u64,
}

#[async_trait]
pub trait LocalAttachmentResolver: Send + Sync {
    /// Resolves an opaque client-owned attachment reference. Implementations
    /// must enforce the OS grant represented by the reference and return only
    /// the referenced bytes; absolute paths are not accepted by this layer.
    async fn resolve(&self, attachment: &LocalAttachmentLocator) -> Result<Vec<u8>, String>;
}

pub struct StoredMainChatContextProvider {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    attachments: Arc<dyn LocalAttachmentResolver>,
}

impl StoredMainChatContextProvider {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        attachments: Arc<dyn LocalAttachmentResolver>,
    ) -> Self {
        Self {
            storage,
            scope,
            attachments,
        }
    }

    async fn load_state(&self, run: &LocalAgentRun) -> StorageResult<StoredMainChatState> {
        let mut operation = LoadStoredMainChatState {
            scope: self.scope.clone(),
            run_id: run.run_id.clone(),
            messages: Vec::new(),
            media: Vec::new(),
        };
        self.storage.transaction(&mut operation).await?;
        Ok(StoredMainChatState {
            messages: operation.messages,
            media: operation.media,
        })
    }
}

#[async_trait]
impl MainChatContextProvider for StoredMainChatContextProvider {
    async fn load_step_context(&self, run: &LocalAgentRun) -> Result<MainChatStepContext, String> {
        if run.owner_user_id != self.scope.owner_user_id
            || run.profile_key != "main_chat"
            || run.owner_entity_type != "conversation"
        {
            return Err("Main Chat context request is outside the provider scope".to_string());
        }
        let state = self
            .load_state(run)
            .await
            .map_err(|error| format!("failed to load durable Main Chat context: {error}"))?;
        main_chat_context_from_state(run, state, self.attachments.as_ref()).await
    }
}

struct StoredMainChatState {
    messages: Vec<AgentMessageStateRecord>,
    media: Vec<MediaStateRecord>,
}

struct LoadStoredMainChatState {
    scope: RecordScope,
    run_id: String,
    messages: Vec<AgentMessageStateRecord>,
    media: Vec<MediaStateRecord>,
}

#[async_trait]
impl StorageTransaction for LoadStoredMainChatState {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut cursor = None;
        loop {
            let page = repositories
                .agent_messages()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            self.messages.extend(
                page.records
                    .into_iter()
                    .filter(|record| record.message.run_id == self.run_id),
            );
            if !advance_cursor(&mut cursor, page.next_cursor)? {
                break;
            }
        }
        let mut cursor = None;
        loop {
            let page = repositories
                .media()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            self.media.extend(page.records.into_iter().filter(|record| {
                record.media_kind == "local_agent_attachment"
                    && record.state.get("run_id").and_then(Value::as_str)
                        == Some(self.run_id.as_str())
            }));
            if !advance_cursor(&mut cursor, page.next_cursor)? {
                break;
            }
        }
        Ok(())
    }
}

fn advance_cursor(cursor: &mut Option<String>, next: Option<String>) -> StorageResult<bool> {
    match next {
        None => Ok(false),
        Some(next) if cursor.as_deref() == Some(next.as_str()) => Err(StorageError::InvalidData {
            reason: "Main Chat context pagination cursor did not advance".to_string(),
        }),
        Some(next) => {
            *cursor = Some(next);
            Ok(true)
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMainChatTurnPayload {
    #[serde(rename = "type")]
    payload_type: String,
    prompt_snapshot: FrozenSnapshot,
    capability_snapshot: FrozenSnapshot,
    project_snapshot: Option<FrozenSnapshot>,
    attachments: Vec<AttachmentManifest>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttachmentManifest {
    pub(crate) attachment_id: String,
    pub(crate) media_type: String,
    pub(crate) payload_digest: String,
    pub(crate) byte_size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredUserInteractionAnswerPayload {
    #[serde(rename = "type")]
    pub(crate) payload_type: String,
    pub(crate) interaction_id: String,
    pub(crate) selected_option_ids: Vec<String>,
    pub(crate) attachments: Vec<AttachmentManifest>,
}

struct UserInputTurn {
    record_id: String,
    sequence: u64,
    text: Option<String>,
    attachments: Vec<AttachmentManifest>,
}

async fn main_chat_context_from_state(
    run: &LocalAgentRun,
    state: StoredMainChatState,
    resolver: &dyn LocalAttachmentResolver,
) -> Result<MainChatStepContext, String> {
    let mut initial = state
        .messages
        .iter()
        .filter(|record| {
            record.message.role == AgentMessageRole::User
                && record.message.message_source == "main_chat"
        })
        .collect::<Vec<_>>();
    if initial.len() != 1 {
        return Err("Main Chat Run must have exactly one frozen initial user message".to_string());
    }
    let initial = &initial.pop().expect("length checked").message;
    if initial.thread_id != run.owner_entity_id {
        return Err("Main Chat initial message does not match the frozen conversation".to_string());
    }
    let payload: StoredMainChatTurnPayload = serde_json::from_value(
        initial
            .structured_payload
            .clone()
            .ok_or_else(|| "Main Chat initial message has no frozen context".to_string())?,
    )
    .map_err(|error| format!("Main Chat frozen context is invalid: {error}"))?;
    if payload.payload_type != "main_chat_turn" {
        return Err("Main Chat frozen context has an unsupported payload type".to_string());
    }
    let prompt_snapshot = MainChatPromptSnapshot::from_frozen(&payload.prompt_snapshot)?;
    let capability_snapshot =
        MainChatCapabilitySnapshot::from_frozen(&payload.capability_snapshot)?;
    let project_snapshot = payload
        .project_snapshot
        .as_ref()
        .map(MainChatProjectSnapshot::from_frozen)
        .transpose()?;
    validate_snapshot_identity(
        run,
        &payload,
        &prompt_snapshot,
        &capability_snapshot,
        project_snapshot.as_ref(),
    )?;

    let mut user_turns = vec![UserInputTurn {
        record_id: initial.record_id.clone(),
        sequence: initial.sequence,
        text: initial.content.clone(),
        attachments: payload.attachments.clone(),
    }];
    for record in state.messages.iter().filter(|record| {
        record.message.role == AgentMessageRole::User
            && record.message.message_source == "user_interaction"
    }) {
        let answer: StoredUserInteractionAnswerPayload =
            serde_json::from_value(
                record.message.structured_payload.clone().ok_or_else(|| {
                    "user interaction answer has no structured payload".to_string()
                })?,
            )
            .map_err(|error| format!("user interaction answer is invalid: {error}"))?;
        if answer.payload_type != "user_interaction_answer"
            || answer.interaction_id.trim().is_empty()
        {
            return Err("user interaction answer identity is invalid".to_string());
        }
        let text = interaction_answer_text(
            record.message.content.as_deref(),
            &answer.selected_option_ids,
        );
        user_turns.push(UserInputTurn {
            record_id: record.message.record_id.clone(),
            sequence: record.message.sequence,
            text,
            attachments: answer.attachments,
        });
    }
    let mut located_turns = Vec::with_capacity(user_turns.len());
    for turn in user_turns {
        let locators = attachment_locators(
            run,
            &turn.record_id,
            &turn.attachments,
            state.media.as_slice(),
        )?;
        located_turns.push((turn, locators));
    }
    let manifest_count = located_turns
        .iter()
        .map(|(_, locators)| locators.len())
        .sum::<usize>();
    if manifest_count != state.media.len() {
        return Err("Local Agent attachment manifest does not match durable media".to_string());
    }
    let latest_assistant_sequence = state
        .messages
        .iter()
        .filter(|record| record.message.role == AgentMessageRole::Assistant)
        .map(|record| record.message.sequence)
        .max()
        .unwrap_or(0);
    let model_input_items = match run.context_strategy {
        ContextStrategy::ProviderNative if run.iteration == 0 => {
            let (turn, locators) = located_turns
                .first()
                .ok_or_else(|| "Main Chat initial input is missing".to_string())?;
            let resolved = resolve_attachments(resolver, locators).await?;
            vec![user_message_item(turn.text.as_deref(), &resolved, true)?]
        }
        ContextStrategy::ProviderNative => {
            let pending = located_turns
                .iter()
                .skip(1)
                .filter(|(turn, _)| turn.sequence > latest_assistant_sequence)
                .max_by_key(|(turn, _)| turn.sequence);
            match pending {
                Some((turn, locators)) => {
                    let resolved = resolve_attachments(resolver, locators).await?;
                    vec![user_message_item(turn.text.as_deref(), &resolved, true)?]
                }
                None => Vec::new(),
            }
        }
        ContextStrategy::MemoryEngine => {
            let locators = located_turns
                .iter()
                .flat_map(|(_, locators)| locators.iter().cloned())
                .collect::<Vec<_>>();
            if locators.is_empty() {
                Vec::new()
            } else {
                let resolved = resolve_attachments(resolver, &locators).await?;
                vec![user_message_item(None, &resolved, false)?]
            }
        }
    };
    let threshold = input_reduction_threshold(run)?;
    Ok(MainChatStepContext {
        prompt_snapshot,
        capability_snapshot,
        project_snapshot,
        model_input_items,
        maximum_output_tokens: run.model_runtime_snapshot.maximum_output_tokens,
        native_compaction_threshold: (run.context_strategy == ContextStrategy::ProviderNative)
            .then_some(threshold),
        memory_engine_active_threshold: (run.context_strategy == ContextStrategy::MemoryEngine)
            .then_some(threshold),
        maximum_summary_attempts: if run.context_strategy == ContextStrategy::MemoryEngine {
            MAXIMUM_SUMMARY_ATTEMPTS
        } else {
            0
        },
    })
}

fn validate_snapshot_identity(
    run: &LocalAgentRun,
    payload: &StoredMainChatTurnPayload,
    prompt: &MainChatPromptSnapshot,
    capabilities: &MainChatCapabilitySnapshot,
    project: Option<&MainChatProjectSnapshot>,
) -> Result<(), String> {
    prompt.validate()?;
    capabilities.validate()?;
    if payload.prompt_snapshot.revision != run.prompt_revision
        || prompt.prompt_revision != run.prompt_revision
        || payload.capability_snapshot.snapshot_id != run.capability_snapshot_ref
        || capabilities.snapshot_ref != run.capability_snapshot_ref
    {
        return Err("Main Chat snapshot identity does not match the frozen Run".to_string());
    }
    match (
        run.project_id.as_deref(),
        project,
        payload.project_snapshot.as_ref(),
    ) {
        (None, None, None) => Ok(()),
        (Some(project_id), Some(project), Some(envelope))
            if project.project_id == project_id
                && project.snapshot_revision == envelope.revision =>
        {
            project.validate()
        }
        _ => Err("Main Chat project snapshot does not match the frozen Run".to_string()),
    }
}

pub(crate) fn attachment_locators(
    run: &LocalAgentRun,
    message_record_id: &str,
    manifests: &[AttachmentManifest],
    media: &[MediaStateRecord],
) -> Result<Vec<LocalAttachmentLocator>, String> {
    let media = media
        .iter()
        .filter(|record| {
            record
                .state
                .get("message_record_id")
                .and_then(Value::as_str)
                == Some(message_record_id)
        })
        .collect::<Vec<_>>();
    if manifests.len() != media.len() {
        return Err("Main Chat attachment manifest does not match durable media".to_string());
    }
    let mut locators = Vec::with_capacity(manifests.len());
    let mut attachment_ids = HashSet::new();
    for manifest in manifests {
        if !attachment_ids.insert(manifest.attachment_id.as_str()) {
            return Err("Local Agent attachment manifest contains duplicate IDs".to_string());
        }
        let record = media
            .iter()
            .find(|record| {
                record.state.get("attachment_id").and_then(Value::as_str)
                    == Some(manifest.attachment_id.as_str())
            })
            .ok_or_else(|| {
                format!(
                    "Local Agent attachment {} is missing",
                    manifest.attachment_id
                )
            })?;
        if record.project_id != run.project_id {
            return Err("Local Agent attachment project scope changed".to_string());
        }
        let locator = LocalAttachmentLocator {
            attachment_id: required_state_string(record, "attachment_id")?,
            media_type: required_state_string(record, "media_type")?,
            payload_reference: required_state_string(record, "payload_reference")?,
            payload_digest: required_state_string(record, "payload_digest")?,
            byte_size: record
                .state
                .get("byte_size")
                .and_then(Value::as_u64)
                .ok_or_else(|| "Local Agent attachment byte size is invalid".to_string())?,
        };
        if locator.attachment_id != manifest.attachment_id
            || locator.media_type != manifest.media_type
            || locator.payload_digest != manifest.payload_digest
            || locator.byte_size != manifest.byte_size
        {
            return Err("Local Agent attachment metadata failed integrity validation".to_string());
        }
        validate_locator(&locator)?;
        locators.push(locator);
    }
    Ok(locators)
}

pub(crate) fn interaction_answer_text(
    content: Option<&str>,
    selected_option_ids: &[String],
) -> Option<String> {
    let content = content.filter(|value| !value.trim().is_empty());
    match (content, selected_option_ids.is_empty()) {
        (Some(content), true) => Some(content.to_string()),
        (Some(content), false) => Some(format!(
            "{content}\n\nSelected option IDs: {}",
            selected_option_ids.join(", ")
        )),
        (None, false) => Some(format!(
            "Selected option IDs: {}",
            selected_option_ids.join(", ")
        )),
        (None, true) => None,
    }
}

fn required_state_string(record: &MediaStateRecord, field: &str) -> Result<String, String> {
    record
        .state
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Local Agent attachment {field} is invalid"))
}

fn validate_locator(locator: &LocalAttachmentLocator) -> Result<(), String> {
    if !matches!(
        locator.media_type.as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    ) {
        return Err(format!(
            "Local Agent attachment {} has unsupported media type {}",
            locator.attachment_id, locator.media_type
        ));
    }
    if locator.byte_size == 0 || locator.byte_size > MAX_RESOLVED_ATTACHMENT_BYTES {
        return Err(format!(
            "Local Agent attachment {} exceeds the visual input size limit",
            locator.attachment_id
        ));
    }
    if looks_like_path(&locator.payload_reference) {
        return Err("Local Agent attachments require opaque payload references".to_string());
    }
    Ok(())
}

fn looks_like_path(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("file://")
        || (value.len() >= 3
            && value.as_bytes()[0].is_ascii_alphabetic()
            && value.as_bytes()[1] == b':'
            && matches!(value.as_bytes()[2], b'\\' | b'/'))
}

pub(crate) async fn resolve_attachments(
    resolver: &dyn LocalAttachmentResolver,
    locators: &[LocalAttachmentLocator],
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut total = 0_u64;
    let mut resolved = Vec::with_capacity(locators.len());
    for locator in locators {
        let bytes = resolver.resolve(locator).await.map_err(|error| {
            format!(
                "failed to resolve attachment {}: {error}",
                locator.attachment_id
            )
        })?;
        let byte_size = u64::try_from(bytes.len())
            .map_err(|_| "resolved attachment size overflowed".to_string())?;
        if byte_size != locator.byte_size
            || format!("sha256:{:x}", Sha256::digest(&bytes)) != locator.payload_digest
        {
            return Err(format!(
                "Local Agent attachment {} content failed integrity validation",
                locator.attachment_id
            ));
        }
        total = total
            .checked_add(byte_size)
            .ok_or_else(|| "Local Agent attachment size overflowed".to_string())?;
        if total > MAX_TOTAL_RESOLVED_ATTACHMENT_BYTES {
            return Err("Local Agent visual attachments exceed the request size limit".to_string());
        }
        resolved.push((locator.media_type.clone(), bytes));
    }
    Ok(resolved)
}

pub(crate) fn user_message_item(
    text: Option<&str>,
    attachments: &[(String, Vec<u8>)],
    require_content: bool,
) -> Result<Value, String> {
    let mut content = Vec::new();
    if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
        content.push(json!({"type": "input_text", "text": text}));
    }
    for (media_type, bytes) in attachments {
        content.push(json!({
            "type": "input_image",
            "image_url": format!("data:{media_type};base64,{}", STANDARD.encode(bytes)),
            "detail": "auto",
        }));
    }
    if content.is_empty() && require_content {
        return Err("Local Agent user message has no model input".to_string());
    }
    Ok(json!({"type": "message", "role": "user", "content": content}))
}

fn input_reduction_threshold(run: &LocalAgentRun) -> Result<u64, String> {
    let context = run.model_runtime_snapshot.context_window_tokens;
    let output = u64::from(run.model_runtime_snapshot.maximum_output_tokens);
    let usable = context
        .checked_sub(output)
        .filter(|value| *value > 0)
        .ok_or_else(|| "model descriptor has no usable input context".to_string())?;
    Ok(usable.saturating_mul(4) / 5)
}
