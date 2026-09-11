// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{require_bounded_json, require_identifier, ProtocolError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentRunStatus {
    Queued,
    ModelReady,
    ModelRunning,
    WaitingToolResult,
    ContinuationReady,
    RetryScheduled,
    Paused,
    NeedsReview,
    Succeeded,
    Failed,
    Cancelled,
}

impl LocalAgentRunStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    ProviderNative,
    MemoryEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentRun {
    pub run_id: String,
    pub profile_key: String,
    pub owner_user_id: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub project_id: Option<String>,
    pub status: LocalAgentRunStatus,
    pub version: u64,
    pub step_seq: u64,
    pub iteration: u32,
    pub retry_count: u32,
    pub model_config_id: String,
    pub model_config_revision: u64,
    pub model_runtime_snapshot: Value,
    pub context_strategy: ContextStrategy,
    pub prompt_revision: String,
    pub capability_snapshot_ref: String,
    pub pending_batch_id: Option<String>,
    pub pending_interaction: Option<Value>,
    pub terminal_outcome: Option<Value>,
    pub deadline_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalAgentRun {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("run_id", self.run_id.as_str()),
            ("profile_key", self.profile_key.as_str()),
            ("owner_user_id", self.owner_user_id.as_str()),
            ("owner_entity_type", self.owner_entity_type.as_str()),
            ("owner_entity_id", self.owner_entity_id.as_str()),
            ("model_config_id", self.model_config_id.as_str()),
            ("prompt_revision", self.prompt_revision.as_str()),
            (
                "capability_snapshot_ref",
                self.capability_snapshot_ref.as_str(),
            ),
        ] {
            require_identifier(field, value)?;
        }
        if let Some(project_id) = &self.project_id {
            require_identifier("project_id", project_id)?;
        }
        if self.version == 0 || self.model_config_revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "run and model configuration revisions must be positive",
            });
        }
        require_bounded_json("model_runtime_snapshot", &self.model_runtime_snapshot)?;
        if let Some(outcome) = &self.terminal_outcome {
            require_bounded_json("terminal_outcome", outcome)?;
        }
        if let Some(interaction) = &self.pending_interaction {
            require_bounded_json("pending_interaction", interaction)?;
        }
        if self.status.is_terminal() != self.terminal_outcome.is_some() {
            return Err(ProtocolError::InvalidState {
                reason: "terminal status and terminal outcome must change together",
            });
        }
        if self.status == LocalAgentRunStatus::WaitingToolResult {
            require_identifier(
                "pending_batch_id",
                self.pending_batch_id.as_deref().unwrap_or_default(),
            )?;
        }
        if self.status == LocalAgentRunStatus::NeedsReview && self.pending_interaction.is_none() {
            return Err(ProtocolError::InvalidState {
                reason: "needs_review requires a pending interaction",
            });
        }
        if self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "updated_at precedes created_at",
            });
        }
        Ok(())
    }
}
