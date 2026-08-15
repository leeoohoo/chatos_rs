// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::{AiTurnReport, AiTurnStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionOutcomeStatus {
    Succeeded,
    Blocked,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskAcceptanceEvidence {
    pub criterion: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub referenced_paths: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskExecutionOutcome {
    pub status: TaskExecutionOutcomeStatus,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking_reason: Option<String>,
    #[serde(default)]
    pub unmet_acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub verification_evidence: Vec<String>,
    #[serde(default)]
    pub acceptance_evidence: Vec<TaskAcceptanceEvidence>,
    #[serde(default)]
    pub referenced_paths: Vec<String>,
    #[serde(default)]
    pub referenced_endpoints: Vec<String>,
}

impl TaskExecutionOutcome {
    pub fn succeeded(summary: impl Into<String>, verification_evidence: Vec<String>) -> Self {
        Self {
            status: TaskExecutionOutcomeStatus::Succeeded,
            summary: summary.into(),
            blocking_reason: None,
            unmet_acceptance_criteria: Vec::new(),
            verification_evidence,
            acceptance_evidence: Vec::new(),
            referenced_paths: Vec::new(),
            referenced_endpoints: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.summary.trim().is_empty() {
            return Err("task execution outcome summary must not be empty".to_string());
        }
        if matches!(
            self.status,
            TaskExecutionOutcomeStatus::Succeeded | TaskExecutionOutcomeStatus::Blocked
        ) && self
            .verification_evidence
            .iter()
            .all(|evidence| evidence.trim().is_empty())
        {
            return Err(
                "task execution outcome must include concrete verification evidence".to_string(),
            );
        }
        if self
            .referenced_paths
            .iter()
            .any(|path| path.trim().is_empty())
        {
            return Err("task execution outcome referenced paths must not be empty".to_string());
        }
        if self
            .referenced_endpoints
            .iter()
            .any(|endpoint| endpoint.trim().is_empty())
        {
            return Err(
                "task execution outcome referenced endpoints must not be empty".to_string(),
            );
        }
        match self.status {
            TaskExecutionOutcomeStatus::Succeeded => {
                if self
                    .blocking_reason
                    .as_deref()
                    .is_some_and(|reason| !reason.trim().is_empty())
                {
                    return Err(
                        "succeeded task execution outcome must not include a blocking reason"
                            .to_string(),
                    );
                }
                if self
                    .unmet_acceptance_criteria
                    .iter()
                    .any(|criterion| !criterion.trim().is_empty())
                {
                    return Err(
                        "succeeded task execution outcome must not include unmet acceptance criteria"
                            .to_string(),
                    );
                }
            }
            TaskExecutionOutcomeStatus::Blocked => {
                if self
                    .blocking_reason
                    .as_deref()
                    .is_none_or(|reason| reason.trim().is_empty())
                {
                    return Err(
                        "blocked task execution outcome must include a blocking reason".to_string(),
                    );
                }
                if self
                    .unmet_acceptance_criteria
                    .iter()
                    .all(|criterion| criterion.trim().is_empty())
                {
                    return Err(
                        "blocked task execution outcome must include unmet acceptance criteria"
                            .to_string(),
                    );
                }
            }
            TaskExecutionOutcomeStatus::Failed | TaskExecutionOutcomeStatus::Cancelled => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunReport {
    pub task_id: String,
    pub run_id: String,
    pub model_config_id: Option<String>,
    pub status: AiTurnStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_outcome: Option<TaskExecutionOutcome>,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub error: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
    pub completed_at: String,
}

impl TaskRunReport {
    pub fn from_ai_report(
        task_id: impl Into<String>,
        run_id: impl Into<String>,
        model_config_id: Option<String>,
        report: AiTurnReport,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            run_id: run_id.into(),
            model_config_id,
            status: report.status,
            execution_outcome: None,
            content: report.content,
            reasoning: report.reasoning,
            error: report.error,
            tool_calls: report.tool_calls,
            finish_reason: report.finish_reason,
            usage: report.usage,
            response_id: report.response_id,
            completed_at: report.completed_at,
        }
    }

    pub fn is_completed(&self) -> bool {
        self.status == AiTurnStatus::Completed
    }

    pub fn is_aborted(&self) -> bool {
        self.status == AiTurnStatus::Aborted
    }

    pub fn user_message(&self) -> String {
        AiTurnReport {
            status: self.status,
            content: self.content.clone(),
            reasoning: self.reasoning.clone(),
            error: self.error.clone(),
            tool_calls: self.tool_calls.clone(),
            finish_reason: self.finish_reason.clone(),
            usage: self.usage.clone(),
            response_id: self.response_id.clone(),
            completed_at: self.completed_at.clone(),
        }
        .user_message()
    }
}
