// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use chatos_client_storage::{
    AgentRunStateRecord, PutRecord, RecordQuery, StorageError, StorageResult, TaskRecord,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{FrozenSnapshot, LocalAgentRun, LocalAgentRunStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const LOCAL_AGENT_TASK_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DurableTaskState {
    pub schema_version: u32,
    pub initial_run_id: String,
    pub current_run_id: String,
    pub run_ids: Vec<String>,
    pub source_thread_id: String,
    pub source_turn_id: String,
    pub project_id: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub model_config_id: String,
    pub model_config_revision: u64,
    pub prompt_snapshot: FrozenSnapshot,
    pub project_snapshot: FrozenSnapshot,
    pub capability_snapshot: FrozenSnapshot,
    pub current_retry_instruction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_status: Option<LocalAgentRunStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_interaction: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_outcome: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_updated_at: Option<DateTime<Utc>>,
}

impl DurableTaskState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        initial_run_id: String,
        source_thread_id: String,
        source_turn_id: String,
        project_id: String,
        objective: String,
        acceptance_criteria: Vec<String>,
        model_config_id: String,
        model_config_revision: u64,
        prompt_snapshot: FrozenSnapshot,
        project_snapshot: FrozenSnapshot,
        capability_snapshot: FrozenSnapshot,
    ) -> StorageResult<Self> {
        let state = Self {
            schema_version: LOCAL_AGENT_TASK_SCHEMA_VERSION,
            current_run_id: initial_run_id.clone(),
            run_ids: vec![initial_run_id.clone()],
            initial_run_id,
            source_thread_id,
            source_turn_id,
            project_id,
            objective,
            acceptance_criteria,
            model_config_id,
            model_config_revision,
            prompt_snapshot,
            project_snapshot,
            capability_snapshot,
            current_retry_instruction: None,
            run_status: None,
            run_version: None,
            step_seq: None,
            iteration: None,
            retry_count: None,
            pending_interaction: None,
            terminal_outcome: None,
            run_updated_at: None,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn from_record(record: &TaskRecord) -> StorageResult<Self> {
        let state: Self = serde_json::from_value(record.state.clone()).map_err(|error| {
            StorageError::InvalidData {
                reason: format!("Task {} state is invalid: {error}", record.metadata.id),
            }
        })?;
        state.validate()?;
        if record.conversation_id.as_deref() != Some(state.source_thread_id.as_str()) {
            return Err(StorageError::InvalidData {
                reason: format!(
                    "Task {} conversation identity does not match source_thread_id",
                    record.metadata.id
                ),
            });
        }
        Ok(state)
    }

    pub fn to_value(&self) -> StorageResult<Value> {
        serde_json::to_value(self).map_err(|error| StorageError::InvalidData {
            reason: format!("Task state cannot be serialized: {error}"),
        })
    }

    pub fn validate(&self) -> StorageResult<()> {
        if self.schema_version != LOCAL_AGENT_TASK_SCHEMA_VERSION {
            return Err(StorageError::InvalidData {
                reason: "Task record does not use the current multi-Run schema".to_string(),
            });
        }
        for (field, value) in [
            ("initial_run_id", self.initial_run_id.as_str()),
            ("current_run_id", self.current_run_id.as_str()),
            ("source_thread_id", self.source_thread_id.as_str()),
            ("source_turn_id", self.source_turn_id.as_str()),
            ("project_id", self.project_id.as_str()),
            ("objective", self.objective.as_str()),
            ("model_config_id", self.model_config_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(StorageError::InvalidData {
                    reason: format!("Task state has no valid {field}"),
                });
            }
        }
        if self.model_config_revision == 0
            || self.acceptance_criteria.is_empty()
            || self
                .acceptance_criteria
                .iter()
                .any(|criterion| criterion.trim().is_empty())
        {
            return Err(StorageError::InvalidData {
                reason: "Task model revision and acceptance criteria must be valid".to_string(),
            });
        }
        let mut unique = BTreeSet::new();
        for run_id in &self.run_ids {
            if run_id.trim().is_empty() || !unique.insert(run_id.as_str()) {
                return Err(StorageError::InvalidData {
                    reason: "Task Run history contains an invalid or duplicate Run ID".to_string(),
                });
            }
        }
        if self.run_ids.first() != Some(&self.initial_run_id)
            || !unique.contains(self.current_run_id.as_str())
        {
            return Err(StorageError::InvalidData {
                reason: "Task Run history does not contain its initial/current Run".to_string(),
            });
        }
        if self
            .current_retry_instruction
            .as_deref()
            .is_some_and(|instruction| instruction.trim().is_empty())
        {
            return Err(StorageError::InvalidData {
                reason: "Task retry instruction must not be empty".to_string(),
            });
        }
        for (field, snapshot) in [
            ("prompt_snapshot", &self.prompt_snapshot),
            ("project_snapshot", &self.project_snapshot),
            ("capability_snapshot", &self.capability_snapshot),
        ] {
            snapshot
                .validate(field)
                .map_err(|error| StorageError::InvalidData {
                    reason: format!("Task {field} is invalid: {error}"),
                })?;
        }
        Ok(())
    }

    pub fn validate_run_identity(&self, task_id: &str, run: &LocalAgentRun) -> StorageResult<()> {
        if run.profile_key != "task_runner"
            || run.owner_entity_type != "task"
            || run.owner_entity_id != task_id
            || run.project_id.as_deref() != Some(self.project_id.as_str())
            || run.model_config_id != self.model_config_id
            || run.model_config_revision != self.model_config_revision
            || run.prompt_revision != self.prompt_snapshot.revision
            || run.capability_snapshot_ref != self.capability_snapshot.snapshot_id
        {
            return Err(StorageError::InvalidData {
                reason: "Task Run does not match its frozen Task identity".to_string(),
            });
        }
        Ok(())
    }

    pub fn same_creation_identity(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.initial_run_id == other.initial_run_id
            && self.current_run_id == other.current_run_id
            && self.run_ids == other.run_ids
            && self.source_thread_id == other.source_thread_id
            && self.source_turn_id == other.source_turn_id
            && self.project_id == other.project_id
            && self.objective == other.objective
            && self.acceptance_criteria == other.acceptance_criteria
            && self.model_config_id == other.model_config_id
            && self.model_config_revision == other.model_config_revision
            && self.prompt_snapshot == other.prompt_snapshot
            && self.project_snapshot == other.project_snapshot
            && self.capability_snapshot == other.capability_snapshot
            && self.current_retry_instruction == other.current_retry_instruction
    }

    pub fn begin_retry(
        &mut self,
        expected_run_id: &str,
        new_run_id: String,
        instruction: Option<String>,
    ) -> StorageResult<()> {
        if self.current_run_id != expected_run_id
            || self.run_ids.iter().any(|run_id| run_id == &new_run_id)
        {
            return Err(StorageError::Conflict { actual_revision: 0 });
        }
        self.current_run_id = new_run_id.clone();
        self.run_ids.push(new_run_id);
        self.current_retry_instruction = instruction;
        self.run_status = None;
        self.run_version = None;
        self.step_seq = None;
        self.iteration = None;
        self.retry_count = None;
        self.pending_interaction = None;
        self.terminal_outcome = None;
        self.run_updated_at = None;
        self.validate()
    }

    fn sync_run(&mut self, run: &LocalAgentRun) {
        self.run_status = Some(run.status);
        self.run_version = Some(run.version);
        self.step_seq = Some(run.step_seq);
        self.iteration = Some(run.iteration);
        self.retry_count = Some(run.retry_count);
        self.pending_interaction = run.pending_interaction.clone();
        self.terminal_outcome = run.terminal_outcome.clone();
        self.run_updated_at = Some(run.updated_at);
    }
}

pub(crate) async fn sync_task_from_run(
    repositories: &mut dyn TransactionRepositories,
    run: &AgentRunStateRecord,
) -> StorageResult<()> {
    if run.run.owner_entity_type != "task" {
        return Ok(());
    }
    let query = RecordQuery {
        scope: run.metadata.scope.clone(),
        id: run.run.owner_entity_id.clone(),
    };
    let mut task =
        repositories
            .tasks()
            .get(&query)
            .await?
            .ok_or_else(|| StorageError::InvalidData {
                reason: format!(
                    "Task Runner run {} has no local Task record {}",
                    run.run.run_id, run.run.owner_entity_id
                ),
            })?;
    let mut state = DurableTaskState::from_record(&task)?;
    state.validate_run_identity(&task.metadata.id, &run.run)?;
    if state.current_run_id != run.run.run_id {
        if state.run_ids.iter().any(|run_id| run_id == &run.run.run_id) {
            return Ok(());
        }
        return Err(StorageError::InvalidData {
            reason: "Task record does not contain its authoritative Run identity".to_string(),
        });
    }
    let status = task_status(run.run.status);
    let before = state.clone();
    state.sync_run(&run.run);
    if task.status == status && before == state {
        return Ok(());
    }
    let revision = task.metadata.revision;
    task.status = status.to_string();
    task.state = state.to_value()?;
    task.metadata.updated_at = run.run.updated_at;
    repositories
        .tasks()
        .put(PutRecord {
            record: task,
            expected_revision: Some(revision),
        })
        .await?;
    Ok(())
}

fn task_status(status: LocalAgentRunStatus) -> &'static str {
    match status {
        LocalAgentRunStatus::Queued => "queued",
        LocalAgentRunStatus::ModelReady
        | LocalAgentRunStatus::ModelRunning
        | LocalAgentRunStatus::WaitingToolResult
        | LocalAgentRunStatus::ContinuationReady
        | LocalAgentRunStatus::RetryScheduled => "running",
        LocalAgentRunStatus::Paused => "paused",
        LocalAgentRunStatus::NeedsReview => "needs_review",
        LocalAgentRunStatus::Succeeded => "succeeded",
        LocalAgentRunStatus::Failed => "failed",
        LocalAgentRunStatus::Cancelled => "cancelled",
    }
}
