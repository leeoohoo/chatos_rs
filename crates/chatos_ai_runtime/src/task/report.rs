use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::{AiTurnReport, AiTurnStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunReport {
    pub task_id: String,
    pub run_id: String,
    pub model_config_id: Option<String>,
    pub status: AiTurnStatus,
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
