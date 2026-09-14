// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_agent_profiles::{
    execute_story_tool, story_tool_definitions, StoryDesignContextProvider, StoryDesignStage,
    StoryDesignState, StoryDesignStepContext, STORY_DESIGN_CAPABILITY_SNAPSHOT_REF,
    STORY_DESIGN_PROFILE_KEY, STORY_DESIGN_PROMPT_REVISION,
};
use chatos_client_storage::{
    AgentMessageStateRecord, AgentRunStateRecord, ClientStorage, ListQuery, PutRecord,
    RecordMetadata, RecordQuery, RecordScope, StorageError, StorageResult, StorageTransaction,
    StoryRecord, StoryRecordKind, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    canonical_json_digest, ApplyStoryDesignCommand, ContextStrategy, CreateStoryDesignCommand,
    LocalAgentRun, LocalAgentRunStatus, LocalStoryDesignApplication, LocalStoryDesignStage,
    ToolEffect, ToolExecutionStatus,
};
use chatos_local_agent_runtime::{
    create_local_agent_run_in_transaction, CreateLocalAgentRunRequest, CreatedLocalAgentRun,
    InitialRunMessage, LocalToolInvocation, LocalToolOutcome, LocalToolRuntime,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::profile_context_support::{input_reduction_threshold, MAXIMUM_SUMMARY_ATTEMPTS};
use crate::story_ipc::story_snapshot;

pub const STORY_DESIGN_RECORD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DurableStoryDesignRecord {
    pub schema_version: u32,
    pub design: StoryDesignState,
    pub tool_receipts: BTreeMap<String, DurableStoryToolReceipt>,
    pub applied_project_revision: Option<u64>,
    pub applied_at: Option<DateTime<Utc>>,
}

impl DurableStoryDesignRecord {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != STORY_DESIGN_RECORD_SCHEMA_VERSION
            || self.tool_receipts.len() > 10_000
            || self.applied_project_revision.is_some() != self.applied_at.is_some()
        {
            return Err("durable story design record is invalid".to_string());
        }
        self.design.validate()?;
        for (invocation_id, receipt) in &self.tool_receipts {
            receipt.validate(invocation_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DurableStoryToolReceipt {
    pub invocation_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub effect: ToolEffect,
    pub arguments: Value,
    pub succeeded: bool,
    pub bounded_result: Value,
}

impl DurableStoryToolReceipt {
    fn validate(&self, key: &str) -> Result<(), String> {
        if self.invocation_id != key
            || self.invocation_id.trim().is_empty()
            || self.tool_call_id.trim().is_empty()
            || self.tool_name.trim().is_empty()
            || !self.arguments.is_object()
        {
            return Err("durable story tool receipt identity is invalid".to_string());
        }
        Ok(())
    }

    fn matches(&self, invocation: &LocalToolInvocation) -> bool {
        self.invocation_id == invocation.invocation_id
            && self.tool_call_id == invocation.tool_call_id
            && self.tool_name == invocation.tool_name
            && self.effect == invocation.effect
            && self.arguments == invocation.arguments
    }

    fn outcome(&self) -> LocalToolOutcome {
        if self.succeeded {
            LocalToolOutcome::succeeded(self.bounded_result.clone())
        } else {
            LocalToolOutcome::failed(self.bounded_result.clone())
        }
    }
}

pub struct StoredStoryDesignContextProvider {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
}

impl StoredStoryDesignContextProvider {
    pub fn new(storage: Arc<dyn ClientStorage>, scope: RecordScope) -> Self {
        Self { storage, scope }
    }
}

#[async_trait]
impl StoryDesignContextProvider for StoredStoryDesignContextProvider {
    async fn load_step_context(
        &self,
        run: &LocalAgentRun,
    ) -> Result<StoryDesignStepContext, String> {
        if run.owner_user_id != self.scope.owner_user_id
            || run.profile_key != STORY_DESIGN_PROFILE_KEY
            || run.owner_entity_type != "story_design"
        {
            return Err("story design context request is outside the provider scope".to_string());
        }
        let mut operation = LoadStoryDesignContext {
            scope: self.scope.clone(),
            run_id: run.run_id.clone(),
            story_record_id: run.owner_entity_id.clone(),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(|error| format!("failed to load durable story design context: {error}"))?;
        let loaded = operation
            .result
            .ok_or_else(|| "story design context query returned no state".to_string())?;
        let durable = decode_story_design(&loaded.story)?;
        validate_run_design_identity(run, &loaded.story, &durable)?;
        let mut initial = loaded
            .messages
            .iter()
            .filter(|record| {
                record.message.run_id == run.run_id
                    && record.message.message_source == "story_design"
            })
            .collect::<Vec<_>>();
        if initial.len() != 1 {
            return Err("story design Run must have one initial goal message".to_string());
        }
        let initial = &initial.pop().expect("length checked").message;
        let goal = durable.design.user_prompt()?;
        if initial.thread_id != durable.design.story_record_id
            || initial.content.as_deref() != Some(goal.as_str())
        {
            return Err("story design initial goal does not match the frozen draft".to_string());
        }
        let model_input_items = match run.context_strategy {
            ContextStrategy::ProviderNative if run.iteration == 0 => {
                vec![json!({
                    "type":"message",
                    "role":"user",
                    "content":[{"type":"input_text","text":goal}]
                })]
            }
            ContextStrategy::ProviderNative | ContextStrategy::MemoryEngine => Vec::new(),
        };
        let threshold = input_reduction_threshold(run)?;
        Ok(StoryDesignStepContext {
            state: durable.design,
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
}

struct LoadedStoryDesignContext {
    story: StoryRecord,
    messages: Vec<AgentMessageStateRecord>,
}

struct LoadStoryDesignContext {
    scope: RecordScope,
    run_id: String,
    story_record_id: String,
    result: Option<LoadedStoryDesignContext>,
}

#[async_trait]
impl StorageTransaction for LoadStoryDesignContext {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let story = repositories
            .stories()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.story_record_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let mut messages = Vec::new();
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
            messages.extend(
                page.records
                    .into_iter()
                    .filter(|record| record.message.run_id == self.run_id),
            );
            let Some(next) = page.next_cursor else {
                break;
            };
            if cursor.as_deref() == Some(next.as_str()) {
                return Err(StorageError::InvalidData {
                    reason: "story message pagination cursor did not advance".to_string(),
                });
            }
            cursor = Some(next);
        }
        self.result = Some(LoadedStoryDesignContext { story, messages });
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CreateStoryDesignRunRequest {
    pub scope: RecordScope,
    pub device_id: String,
    pub causation_id: String,
    pub command: CreateStoryDesignCommand,
    pub model_runtime_snapshot: chatos_local_agent_protocol::ModelRuntimeDescriptor,
    pub now: DateTime<Utc>,
}

pub async fn create_story_design_run(
    storage: &dyn ClientStorage,
    request: CreateStoryDesignRunRequest,
) -> StorageResult<CreatedLocalAgentRun> {
    let mut operation = CreateStoryDesignRunOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "story design Run creation returned no result".to_string(),
    })
}

struct CreateStoryDesignRunOperation {
    request: Option<CreateStoryDesignRunRequest>,
    result: Option<CreatedLocalAgentRun>,
}

#[async_trait]
impl StorageTransaction for CreateStoryDesignRunOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "story design creation request was already consumed".to_string(),
        })?;
        request.command.validate().map_err(invalid_protocol)?;
        let command = request.command;
        let query = RecordQuery {
            scope: request.scope.clone(),
            id: command.story_record_id.clone(),
        };
        let existing = repositories.stories().get(&query).await?;
        let (story, durable) = if let Some(story) = existing {
            let durable = decode_story_design(&story).map_err(invalid_story)?;
            validate_creation_replay(&story, &durable, &command)?;
            (story, durable)
        } else {
            let project = repositories
                .stories()
                .get(&RecordQuery {
                    scope: request.scope.clone(),
                    id: format!("project:{}", command.project_id),
                })
                .await?
                .ok_or(StorageError::NotFound)?;
            let durable = prepare_design(&project, &command)?;
            let story = repositories
                .stories()
                .put(PutRecord {
                    record: StoryRecord {
                        metadata: RecordMetadata {
                            id: command.story_record_id.clone(),
                            scope: request.scope.clone(),
                            origin_device_id: request.device_id.clone(),
                            revision: 0,
                            created_at: request.now,
                            updated_at: request.now,
                        },
                        project_id: command.project_id.clone(),
                        kind: StoryRecordKind::AgentRun,
                        status: Some("active".to_string()),
                        state: serde_json::to_value(&durable).map_err(|error| {
                            StorageError::InvalidData {
                                reason: format!("story design state could not be encoded: {error}"),
                            }
                        })?,
                    },
                    expected_revision: None,
                })
                .await?;
            (story, durable)
        };
        validate_story_record_identity(&story, &durable.design).map_err(invalid_story)?;
        let goal = durable.design.user_prompt().map_err(invalid_story)?;
        let run = create_local_agent_run_in_transaction(
            repositories,
            CreateLocalAgentRunRequest {
                scope: request.scope,
                run_id: command.run_id.clone(),
                profile_key: STORY_DESIGN_PROFILE_KEY.to_string(),
                owner_entity_type: "story_design".to_string(),
                owner_entity_id: command.story_record_id.clone(),
                project_id: Some(command.project_id),
                model_runtime_snapshot: request.model_runtime_snapshot,
                prompt_revision: STORY_DESIGN_PROMPT_REVISION.to_string(),
                capability_snapshot_ref: STORY_DESIGN_CAPABILITY_SNAPSHOT_REF.to_string(),
                origin_device_id: request.device_id,
                causation_id: request.causation_id,
                deadline_at: None,
                initial_message: Some(InitialRunMessage {
                    record_id: format!("story-design-message:{}", command.run_id),
                    turn_id: command.run_id,
                    content: Some(goal),
                    structured_payload: Some(json!({
                        "type":"story_design",
                        "story_record_id":command.story_record_id,
                        "base_project_revision":durable.design.base_project_revision,
                        "base_project_digest":durable.design.base_project_digest
                    })),
                    message_source: "story_design".to_string(),
                }),
                initial_attachments: Vec::new(),
                now: request.now,
            },
        )
        .await?;
        self.result = Some(run);
        Ok(())
    }
}

fn prepare_design(
    project: &StoryRecord,
    command: &CreateStoryDesignCommand,
) -> StorageResult<DurableStoryDesignRecord> {
    if project.kind != StoryRecordKind::Project
        || project.project_id != command.project_id
        || project.metadata.revision != command.expected_project_revision
        || canonical_json_digest(&project.state) != command.base_project_digest
    {
        return Err(StorageError::Conflict {
            actual_revision: project.metadata.revision,
        });
    }
    let stage = stage(command.stage);
    let mut draft = project.state.clone();
    let segments = draft
        .get_mut("segments")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| StorageError::InvalidData {
            reason: "story project segments are invalid".to_string(),
        })?;
    match stage {
        StoryDesignStage::Outline if !segments.is_empty() => {
            return Err(StorageError::InvalidData {
                reason: "outline story design requires a project without segments".to_string(),
            });
        }
        StoryDesignStage::Refine => {
            for target in &command.target_ids {
                let segment = segments
                    .iter_mut()
                    .find(|value| value.get("id").and_then(Value::as_str) == Some(target))
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| StorageError::InvalidData {
                        reason: "story refine target is absent".to_string(),
                    })?;
                if segment.get("attempt").is_some_and(|value| !value.is_null())
                    || segment.get("video").is_some_and(|value| !value.is_null())
                {
                    return Err(StorageError::InvalidData {
                        reason: "story refine target has an active or completed media job"
                            .to_string(),
                    });
                }
                segment.insert("detail".to_string(), Value::Null);
                clear_confirmed_image(segment, "firstFrames")?;
                clear_confirmed_image(segment, "lastFrames")?;
                segment.insert("useLastFrameForVideo".to_string(), Value::Bool(false));
            }
        }
        _ => {}
    }
    let design = StoryDesignState {
        schema_version: 1,
        story_record_id: command.story_record_id.clone(),
        project_id: command.project_id.clone(),
        base_project_revision: command.expected_project_revision,
        base_project_digest: command.base_project_digest.clone(),
        stage,
        target_ids: command.target_ids.clone(),
        draft,
        read_through: 0,
        read_segment_ids: Vec::new(),
    };
    design.validate().map_err(invalid_story)?;
    Ok(DurableStoryDesignRecord {
        schema_version: STORY_DESIGN_RECORD_SCHEMA_VERSION,
        design,
        tool_receipts: BTreeMap::new(),
        applied_project_revision: None,
        applied_at: None,
    })
}

fn clear_confirmed_image(
    segment: &mut serde_json::Map<String, Value>,
    field: &str,
) -> StorageResult<()> {
    segment
        .get_mut(field)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| StorageError::InvalidData {
            reason: format!("story segment {field} is invalid"),
        })?
        .insert("confirmedImageID".to_string(), Value::Null);
    Ok(())
}

fn validate_creation_replay(
    story: &StoryRecord,
    durable: &DurableStoryDesignRecord,
    command: &CreateStoryDesignCommand,
) -> StorageResult<()> {
    let expected_stage = stage(command.stage);
    if story.kind != StoryRecordKind::AgentRun
        || story.project_id != command.project_id
        || durable.design.story_record_id != command.story_record_id
        || durable.design.project_id != command.project_id
        || durable.design.base_project_revision != command.expected_project_revision
        || durable.design.base_project_digest != command.base_project_digest
        || durable.design.stage != expected_stage
        || durable.design.target_ids != command.target_ids
        || durable.applied_at.is_some()
    {
        return Err(StorageError::Conflict {
            actual_revision: story.metadata.revision,
        });
    }
    Ok(())
}

pub struct StoryDesignLocalToolRuntime {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
}

impl StoryDesignLocalToolRuntime {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        device_id: impl Into<String>,
    ) -> Self {
        Self {
            storage,
            scope,
            device_id: device_id.into(),
        }
    }
}

#[async_trait]
impl LocalToolRuntime for StoryDesignLocalToolRuntime {
    async fn execute(
        &self,
        invocation: LocalToolInvocation,
        cancellation: CancellationToken,
    ) -> Result<LocalToolOutcome, String> {
        if cancellation.is_cancelled() {
            return Err("story design tool execution was cancelled".to_string());
        }
        let mut operation = ExecuteStoryDesignTool {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            invocation: Some(invocation),
            result: None,
            now: Utc::now(),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(|error| format!("story design tool transaction failed: {error}"))?;
        if cancellation.is_cancelled() {
            return Err("story design tool execution was cancelled".to_string());
        }
        operation
            .result
            .ok_or_else(|| "story design tool transaction returned no outcome".to_string())
    }
}

struct ExecuteStoryDesignTool {
    scope: RecordScope,
    device_id: String,
    invocation: Option<LocalToolInvocation>,
    result: Option<LocalToolOutcome>,
    now: DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for ExecuteStoryDesignTool {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let invocation = self.invocation.take().ok_or(StorageError::Transaction {
            reason: "story tool invocation was already consumed".to_string(),
        })?;
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: invocation.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let mut story = repositories
            .stories()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: run.run.owner_entity_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let mut durable = decode_story_design(&story).map_err(invalid_story)?;
        validate_tool_identity(&run, &story, &durable, &invocation)?;
        if let Some(receipt) = durable.tool_receipts.get(&invocation.invocation_id) {
            if !receipt.matches(&invocation) {
                return Err(StorageError::Conflict {
                    actual_revision: story.metadata.revision,
                });
            }
            self.result = Some(receipt.outcome());
            return Ok(());
        }
        let outcome = match execute_story_tool(
            &mut durable.design,
            invocation.tool_name.as_str(),
            &invocation.arguments,
        ) {
            Ok(result) => LocalToolOutcome::succeeded(json!({
                "result":result.bounded_result,
                "changed":result.changed
            })),
            Err(error) => LocalToolOutcome::failed(json!({"error":error})),
        };
        durable.tool_receipts.insert(
            invocation.invocation_id.clone(),
            DurableStoryToolReceipt {
                invocation_id: invocation.invocation_id,
                tool_call_id: invocation.tool_call_id,
                tool_name: invocation.tool_name,
                effect: invocation.effect,
                arguments: invocation.arguments,
                succeeded: outcome.status == ToolExecutionStatus::Succeeded,
                bounded_result: outcome.bounded_result.clone(),
            },
        );
        durable.validate().map_err(invalid_story)?;
        let revision = story.metadata.revision;
        story.metadata.origin_device_id = self.device_id.clone();
        story.metadata.updated_at = self.now;
        story.state =
            serde_json::to_value(&durable).map_err(|error| StorageError::InvalidData {
                reason: format!("story design state could not be encoded: {error}"),
            })?;
        repositories
            .stories()
            .put(PutRecord {
                record: story,
                expected_revision: Some(revision),
            })
            .await?;
        self.result = Some(outcome);
        Ok(())
    }
}

fn validate_tool_identity(
    run: &AgentRunStateRecord,
    story: &StoryRecord,
    durable: &DurableStoryDesignRecord,
    invocation: &LocalToolInvocation,
) -> StorageResult<()> {
    validate_run_design_identity(&run.run, story, durable).map_err(invalid_story)?;
    let project_id = invocation
        .project_id
        .as_deref()
        .ok_or_else(|| StorageError::InvalidData {
            reason: "story design tool has no frozen project ID".to_string(),
        })?;
    let definition = story_tool_definitions(durable.design.stage)
        .into_iter()
        .find(|definition| definition.name == invocation.tool_name)
        .ok_or_else(|| StorageError::InvalidData {
            reason: "story design tool is outside the frozen stage".to_string(),
        })?;
    if run.metadata.id != invocation.run_id
        || run.metadata.scope != story.metadata.scope
        || run.run.project_id.as_deref() != Some(project_id)
        || durable.design.project_id != project_id
        || run.run.capability_snapshot_ref != invocation.capability_snapshot_ref
        || invocation.capability_snapshot_ref != STORY_DESIGN_CAPABILITY_SNAPSHOT_REF
        || invocation.effect != definition.effect
        || definition.effect == ToolEffect::Terminal
    {
        return Err(StorageError::InvalidData {
            reason: "story design tool invocation does not match its frozen Run".to_string(),
        });
    }
    Ok(())
}

pub struct ProfileRoutingLocalToolRuntime {
    story: Arc<dyn LocalToolRuntime>,
    fallback: Arc<dyn LocalToolRuntime>,
}

impl ProfileRoutingLocalToolRuntime {
    pub fn new(story: Arc<dyn LocalToolRuntime>, fallback: Arc<dyn LocalToolRuntime>) -> Self {
        Self { story, fallback }
    }
}

#[async_trait]
impl LocalToolRuntime for ProfileRoutingLocalToolRuntime {
    async fn execute(
        &self,
        invocation: LocalToolInvocation,
        cancellation: CancellationToken,
    ) -> Result<LocalToolOutcome, String> {
        if invocation.tool_name.starts_with("story_") {
            self.story.execute(invocation, cancellation).await
        } else {
            self.fallback.execute(invocation, cancellation).await
        }
    }
}

pub async fn apply_story_design(
    storage: &dyn ClientStorage,
    scope: RecordScope,
    device_id: String,
    command: ApplyStoryDesignCommand,
    now: DateTime<Utc>,
) -> StorageResult<LocalStoryDesignApplication> {
    command.validate().map_err(invalid_protocol)?;
    let mut operation = ApplyStoryDesign {
        scope,
        device_id,
        command: Some(command),
        now,
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "story design apply returned no result".to_string(),
    })
}

struct ApplyStoryDesign {
    scope: RecordScope,
    device_id: String,
    command: Option<ApplyStoryDesignCommand>,
    now: DateTime<Utc>,
    result: Option<LocalStoryDesignApplication>,
}

#[async_trait]
impl StorageTransaction for ApplyStoryDesign {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "story design apply command was already consumed".to_string(),
        })?;
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: command.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let mut story = repositories
            .stories()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: command.story_record_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let mut durable = decode_story_design(&story).map_err(invalid_story)?;
        validate_run_design_identity(&run.run, &story, &durable).map_err(invalid_story)?;
        let mut project = repositories
            .stories()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: format!("project:{}", command.project_id),
            })
            .await?
            .ok_or(StorageError::NotFound)?;

        if let Some(applied_revision) = durable.applied_project_revision {
            if applied_revision == project.metadata.revision
                && canonical_json_digest(&project.state)
                    == canonical_json_digest(&durable.design.draft)
            {
                self.result = Some(LocalStoryDesignApplication {
                    project: story_snapshot(project)?,
                    design: story_snapshot(story)?,
                });
                return Ok(());
            }
            return Err(StorageError::Conflict {
                actual_revision: project.metadata.revision,
            });
        }

        let outcome = run
            .run
            .terminal_outcome
            .as_ref()
            .filter(|_| run.run.status == LocalAgentRunStatus::Succeeded)
            .ok_or_else(|| StorageError::InvalidData {
                reason: "story design Run has not succeeded".to_string(),
            })?;
        if command.project_id != durable.design.project_id
            || command.expected_story_revision != story.metadata.revision
            || command.expected_project_revision != project.metadata.revision
            || command.expected_project_revision != durable.design.base_project_revision
            || canonical_json_digest(&project.state) != durable.design.base_project_digest
            || outcome.get("kind").and_then(Value::as_str) != Some("story_design")
            || outcome.get("story_record_id").and_then(Value::as_str)
                != Some(command.story_record_id.as_str())
            || outcome.get("draft_digest").and_then(Value::as_str)
                != Some(canonical_json_digest(&durable.design.draft).as_str())
        {
            return Err(StorageError::Conflict {
                actual_revision: project.metadata.revision,
            });
        }
        let project_revision = project.metadata.revision;
        project.metadata.origin_device_id = self.device_id.clone();
        project.metadata.updated_at = self.now;
        project.state = durable.design.draft.clone();
        let project = repositories
            .stories()
            .put(PutRecord {
                record: project,
                expected_revision: Some(project_revision),
            })
            .await?;
        let story_revision = story.metadata.revision;
        durable.applied_project_revision = Some(project.metadata.revision);
        durable.applied_at = Some(self.now);
        durable.validate().map_err(invalid_story)?;
        story.metadata.origin_device_id = self.device_id.clone();
        story.metadata.updated_at = self.now;
        story.status = Some("applied".to_string());
        story.state =
            serde_json::to_value(&durable).map_err(|error| StorageError::InvalidData {
                reason: format!("applied story design could not be encoded: {error}"),
            })?;
        let story = repositories
            .stories()
            .put(PutRecord {
                record: story,
                expected_revision: Some(story_revision),
            })
            .await?;
        self.result = Some(LocalStoryDesignApplication {
            project: story_snapshot(project)?,
            design: story_snapshot(story)?,
        });
        Ok(())
    }
}

fn validate_run_design_identity(
    run: &LocalAgentRun,
    story: &StoryRecord,
    durable: &DurableStoryDesignRecord,
) -> Result<(), String> {
    durable.validate()?;
    validate_story_record_identity(story, &durable.design)?;
    if run.profile_key != STORY_DESIGN_PROFILE_KEY
        || run.owner_entity_type != "story_design"
        || run.owner_entity_id != durable.design.story_record_id
        || run.project_id.as_deref() != Some(durable.design.project_id.as_str())
        || run.prompt_revision != STORY_DESIGN_PROMPT_REVISION
        || run.capability_snapshot_ref != STORY_DESIGN_CAPABILITY_SNAPSHOT_REF
    {
        return Err("durable story design does not match its frozen Run".to_string());
    }
    Ok(())
}

fn validate_story_record_identity(
    story: &StoryRecord,
    design: &StoryDesignState,
) -> Result<(), String> {
    if story.kind != StoryRecordKind::AgentRun
        || story.metadata.id != design.story_record_id
        || story.project_id != design.project_id
    {
        return Err("story design record identity is invalid".to_string());
    }
    Ok(())
}

fn decode_story_design(story: &StoryRecord) -> Result<DurableStoryDesignRecord, String> {
    let durable: DurableStoryDesignRecord = serde_json::from_value(story.state.clone())
        .map_err(|error| format!("durable story design state is invalid: {error}"))?;
    durable.validate()?;
    Ok(durable)
}

fn stage(stage: LocalStoryDesignStage) -> StoryDesignStage {
    match stage {
        LocalStoryDesignStage::Outline => StoryDesignStage::Outline,
        LocalStoryDesignStage::Refine => StoryDesignStage::Refine,
    }
}

fn invalid_story(reason: String) -> StorageError {
    StorageError::InvalidData { reason }
}

fn invalid_protocol(error: chatos_local_agent_protocol::ProtocolError) -> StorageError {
    StorageError::InvalidData {
        reason: error.to_string(),
    }
}
