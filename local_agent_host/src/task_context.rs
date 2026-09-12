// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_agent_profiles::{
    TaskRunnerCapabilitySnapshot, TaskRunnerContextProvider, TaskRunnerProjectSnapshot,
    TaskRunnerPromptSnapshot, TaskRunnerStepContext, TaskRunnerToolReceipt,
    TaskRunnerToolReceiptStatus,
};
use chatos_client_storage::{
    AgentMessageStateRecord, ClientStorage, ListQuery, MediaStateRecord, RecordQuery, RecordScope,
    StorageError, StorageResult, StorageTransaction, TaskRecord, ToolExecutionStateRecord,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessageRole, ContextStrategy, FrozenSnapshot, LocalAgentRun, ToolExecutionStatus,
};

use crate::{
    attachment_locators, interaction_answer_text, resolve_attachments, user_message_item,
    LocalAttachmentResolver, StoredUserInteractionAnswerPayload,
};

const MAXIMUM_SUMMARY_ATTEMPTS: u8 = 8;

pub struct StoredTaskRunnerContextProvider {
    storage: std::sync::Arc<dyn ClientStorage>,
    scope: RecordScope,
    attachments: std::sync::Arc<dyn LocalAttachmentResolver>,
}

impl StoredTaskRunnerContextProvider {
    pub fn new(
        storage: std::sync::Arc<dyn ClientStorage>,
        scope: RecordScope,
        attachments: std::sync::Arc<dyn LocalAttachmentResolver>,
    ) -> Self {
        Self {
            storage,
            scope,
            attachments,
        }
    }

    async fn load_state(&self, run: &LocalAgentRun) -> StorageResult<StoredTaskContextState> {
        let mut operation = LoadStoredTaskContext {
            scope: self.scope.clone(),
            task_id: run.owner_entity_id.clone(),
            run_id: run.run_id.clone(),
            result: None,
        };
        self.storage.transaction(&mut operation).await?;
        operation.result.ok_or(StorageError::Transaction {
            reason: "Task Runner context query returned no state".to_string(),
        })
    }
}

#[async_trait]
impl TaskRunnerContextProvider for StoredTaskRunnerContextProvider {
    async fn load_step_context(
        &self,
        run: &LocalAgentRun,
    ) -> Result<TaskRunnerStepContext, String> {
        if run.owner_user_id != self.scope.owner_user_id
            || run.profile_key != "task_runner"
            || run.owner_entity_type != "task"
        {
            return Err("Task Runner context request is outside the provider scope".to_string());
        }
        let state = self
            .load_state(run)
            .await
            .map_err(|error| format!("failed to load durable Task Runner context: {error}"))?;
        task_runner_context_from_state(run, state, self.attachments.as_ref()).await
    }
}

struct StoredTaskContextState {
    task: TaskRecord,
    tool_executions: Vec<ToolExecutionStateRecord>,
    messages: Vec<AgentMessageStateRecord>,
    media: Vec<MediaStateRecord>,
}

struct LoadStoredTaskContext {
    scope: RecordScope,
    task_id: String,
    run_id: String,
    result: Option<StoredTaskContextState>,
}

#[async_trait]
impl StorageTransaction for LoadStoredTaskContext {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let task = repositories
            .tasks()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.task_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let mut tool_executions = Vec::new();
        let mut messages = Vec::new();
        let mut media = Vec::new();
        let mut cursor = None;
        loop {
            let page = repositories
                .tool_executions()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            tool_executions.extend(
                page.records
                    .into_iter()
                    .filter(|record| record.execution.run_id == self.run_id),
            );
            let Some(next) = page.next_cursor else {
                break;
            };
            if cursor.as_deref() == Some(next.as_str()) {
                return Err(StorageError::InvalidData {
                    reason: "tool execution pagination cursor did not advance".to_string(),
                });
            }
            cursor = Some(next);
        }
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
                    reason: "message pagination cursor did not advance".to_string(),
                });
            }
            cursor = Some(next);
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
            media.extend(page.records.into_iter().filter(|record| {
                record.media_kind == "local_agent_attachment"
                    && record
                        .state
                        .get("run_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(self.run_id.as_str())
            }));
            let Some(next) = page.next_cursor else {
                break;
            };
            if cursor.as_deref() == Some(next.as_str()) {
                return Err(StorageError::InvalidData {
                    reason: "media pagination cursor did not advance".to_string(),
                });
            }
            cursor = Some(next);
        }
        self.result = Some(StoredTaskContextState {
            task,
            tool_executions,
            messages,
            media,
        });
        Ok(())
    }
}

async fn task_runner_context_from_state(
    run: &LocalAgentRun,
    state: StoredTaskContextState,
    resolver: &dyn LocalAttachmentResolver,
) -> Result<TaskRunnerStepContext, String> {
    let task = state.task;
    if task.metadata.id != run.owner_entity_id
        || task.metadata.scope.owner_user_id != run.owner_user_id
        || task.state.get("run_id").and_then(serde_json::Value::as_str) != Some(run.run_id.as_str())
        || task
            .state
            .get("project_id")
            .and_then(serde_json::Value::as_str)
            != run.project_id.as_deref()
        || task
            .state
            .get("model_config_id")
            .and_then(serde_json::Value::as_str)
            != Some(run.model_config_id.as_str())
        || task
            .state
            .get("model_config_revision")
            .and_then(serde_json::Value::as_u64)
            != Some(run.model_config_revision)
    {
        return Err("durable Task identity does not match the frozen Task Runner run".to_string());
    }
    let prompt_snapshot = snapshot_from_task(&task, "prompt_snapshot")?;
    let project_snapshot = snapshot_from_task(&task, "project_snapshot")?;
    let capability_snapshot = snapshot_from_task(&task, "capability_snapshot")?;
    if prompt_snapshot.revision != run.prompt_revision
        || capability_snapshot.snapshot_id != run.capability_snapshot_ref
    {
        return Err(
            "durable Task snapshot references do not match the Task Runner run".to_string(),
        );
    }
    let prompt: TaskRunnerPromptSnapshot = snapshot_payload(&prompt_snapshot, "prompt_snapshot")?;
    let project: TaskRunnerProjectSnapshot =
        snapshot_payload(&project_snapshot, "project_snapshot")?;
    let capability: TaskRunnerCapabilitySnapshot =
        snapshot_payload(&capability_snapshot, "capability_snapshot")?;
    if prompt.prompt_revision != prompt_snapshot.revision
        || project.snapshot_revision != project_snapshot.revision
        || capability.snapshot_ref != capability_snapshot.snapshot_id
    {
        return Err(
            "frozen Task snapshot payload identity does not match its reference".to_string(),
        );
    }
    let objective = required_string(&task, "objective")?;
    let acceptance_criteria = task
        .state
        .get("acceptance_criteria")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "durable Task has no acceptance criteria".to_string())?
        .iter()
        .map(|criterion| {
            criterion
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| "durable Task acceptance criteria are invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let model_input_items = interaction_model_input_items(
        run,
        state.messages.as_slice(),
        state.media.as_slice(),
        resolver,
    )
    .await?;
    let mut receipts = state
        .tool_executions
        .into_iter()
        .filter_map(tool_receipt)
        .collect::<Vec<_>>();
    receipts.sort_by(|left, right| left.receipt_id.cmp(&right.receipt_id));
    let threshold = input_reduction_threshold(run)?;
    Ok(TaskRunnerStepContext {
        project_snapshot: project,
        objective,
        acceptance_criteria,
        prompt_snapshot: prompt,
        capability_snapshot: capability,
        model_input_items,
        tool_receipts: receipts,
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

async fn interaction_model_input_items(
    run: &LocalAgentRun,
    messages: &[AgentMessageStateRecord],
    media: &[MediaStateRecord],
    resolver: &dyn LocalAttachmentResolver,
) -> Result<Vec<serde_json::Value>, String> {
    let mut turns = Vec::new();
    for record in messages.iter().filter(|record| {
        record.message.role == AgentMessageRole::User
            && record.message.message_source == "user_interaction"
    }) {
        let answer: StoredUserInteractionAnswerPayload = serde_json::from_value(
            record
                .message
                .structured_payload
                .clone()
                .ok_or_else(|| "Task Runner interaction answer has no payload".to_string())?,
        )
        .map_err(|error| format!("Task Runner interaction answer is invalid: {error}"))?;
        if answer.payload_type != "user_interaction_answer"
            || answer.interaction_id.trim().is_empty()
        {
            return Err("Task Runner interaction answer identity is invalid".to_string());
        }
        let text = interaction_answer_text(
            record.message.content.as_deref(),
            &answer.selected_option_ids,
        );
        let locators =
            attachment_locators(run, &record.message.record_id, &answer.attachments, media)?;
        turns.push((record.message.sequence, text, locators));
    }
    if turns
        .iter()
        .map(|(_, _, locators)| locators.len())
        .sum::<usize>()
        != media.len()
    {
        return Err("Task Runner attachment manifest does not match durable media".to_string());
    }
    match run.context_strategy {
        ContextStrategy::ProviderNative => {
            let latest_assistant = messages
                .iter()
                .filter(|record| record.message.role == AgentMessageRole::Assistant)
                .map(|record| record.message.sequence)
                .max()
                .unwrap_or(0);
            let pending = turns
                .iter()
                .filter(|(sequence, _, _)| *sequence > latest_assistant)
                .max_by_key(|(sequence, _, _)| *sequence);
            match pending {
                Some((_, text, locators)) => {
                    let resolved = resolve_attachments(resolver, locators).await?;
                    Ok(vec![user_message_item(text.as_deref(), &resolved, true)?])
                }
                None => Ok(Vec::new()),
            }
        }
        ContextStrategy::MemoryEngine => {
            let locators = turns
                .into_iter()
                .flat_map(|(_, _, locators)| locators)
                .collect::<Vec<_>>();
            if locators.is_empty() {
                Ok(Vec::new())
            } else {
                let resolved = resolve_attachments(resolver, &locators).await?;
                Ok(vec![user_message_item(None, &resolved, false)?])
            }
        }
    }
}

fn snapshot_from_task(task: &TaskRecord, field: &'static str) -> Result<FrozenSnapshot, String> {
    let snapshot: FrozenSnapshot = serde_json::from_value(
        task.state
            .get(field)
            .cloned()
            .ok_or_else(|| format!("durable Task has no {field}"))?,
    )
    .map_err(|error| format!("durable Task {field} is invalid: {error}"))?;
    snapshot
        .validate(field)
        .map_err(|error| format!("durable Task {field} failed integrity validation: {error}"))?;
    Ok(snapshot)
}

fn snapshot_payload<T: serde::de::DeserializeOwned>(
    snapshot: &FrozenSnapshot,
    field: &str,
) -> Result<T, String> {
    serde_json::from_value(snapshot.payload.clone()).map_err(|error| {
        format!("{field} payload does not match the Task Runner contract: {error}")
    })
}

fn required_string(task: &TaskRecord, field: &str) -> Result<String, String> {
    task.state
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("durable Task has no valid {field}"))
}

fn tool_receipt(record: ToolExecutionStateRecord) -> Option<TaskRunnerToolReceipt> {
    let execution = record.execution;
    let status = match execution.status {
        ToolExecutionStatus::Succeeded => TaskRunnerToolReceiptStatus::Succeeded,
        ToolExecutionStatus::Failed => TaskRunnerToolReceiptStatus::Failed,
        ToolExecutionStatus::OutcomeUnknown => TaskRunnerToolReceiptStatus::OutcomeUnknown,
        ToolExecutionStatus::Requested | ToolExecutionStatus::Started => return None,
    };
    let verification = status == TaskRunnerToolReceiptStatus::Succeeded
        && execution
            .bounded_result
            .as_ref()
            .and_then(|result| result.get("verification"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
    let summary = execution
        .bounded_result
        .as_ref()
        .and_then(|result| result.get("summary"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| execution.bounded_result.as_ref().map(ToString::to_string))
        .unwrap_or_else(|| "Tool outcome is unknown and requires review.".to_string());
    Some(TaskRunnerToolReceipt {
        receipt_id: execution.invocation_id,
        tool_call_id: execution.tool_call_id,
        tool_name: execution.tool_name,
        status,
        verification,
        summary,
    })
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
