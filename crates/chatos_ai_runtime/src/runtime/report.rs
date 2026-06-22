use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AiRuntimeResult {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
}

impl AiRuntimeResult {
    pub fn into_report(self) -> AiTurnReport {
        AiTurnReport::completed(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTurnStatus {
    Completed,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTurnReport {
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

impl AiTurnReport {
    pub fn completed(result: AiRuntimeResult) -> Self {
        Self {
            status: AiTurnStatus::Completed,
            content: Some(result.content),
            reasoning: result.reasoning,
            error: None,
            tool_calls: result.tool_calls,
            finish_reason: result.finish_reason,
            usage: result.usage,
            response_id: result.response_id,
            completed_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        let error = error.into();
        let status = if error == "aborted" {
            AiTurnStatus::Aborted
        } else {
            AiTurnStatus::Failed
        };
        Self {
            status,
            content: None,
            reasoning: None,
            error: Some(error),
            tool_calls: None,
            finish_reason: None,
            usage: None,
            response_id: None,
            completed_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn aborted() -> Self {
        Self::failed("aborted")
    }

    pub fn is_completed(&self) -> bool {
        self.status == AiTurnStatus::Completed
    }

    pub fn is_aborted(&self) -> bool {
        self.status == AiTurnStatus::Aborted
    }

    pub fn user_message(&self) -> String {
        match self.status {
            AiTurnStatus::Completed => {
                if let Some(content) = self
                    .content
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    format!("任务已完成。\n\n{content}")
                } else {
                    "任务已完成。".to_string()
                }
            }
            AiTurnStatus::Failed => {
                let error = self
                    .error
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("未知错误");
                format!("任务执行失败：{error}")
            }
            AiTurnStatus::Aborted => "任务已取消。".to_string(),
        }
    }
}
