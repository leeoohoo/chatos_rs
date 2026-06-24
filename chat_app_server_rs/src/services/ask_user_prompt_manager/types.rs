use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT: u64 = 86_400_000;
pub const ASK_USER_PROMPT_TIMEOUT_ERR: &str = "ask_user_prompt_timeout";
pub const ASK_USER_PROMPT_NOT_FOUND_ERR: &str = "ask_user_prompt_not_found";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AskUserPromptStatus {
    Pending,
    Ok,
    Canceled,
    Timeout,
    Failed,
}

impl AskUserPromptStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ok => "ok",
            Self::Canceled => "canceled",
            Self::Timeout => "timeout",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "ok" | "submitted" => Some(Self::Ok),
            "canceled" | "cancelled" => Some(Self::Canceled),
            "timeout" | "timed_out" => Some(Self::Timeout),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserPromptPayload {
    pub prompt_id: String,
    #[serde(rename = "conversation_id")]
    pub conversation_id: String,
    pub conversation_turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub message: String,
    #[serde(default = "default_allow_cancel")]
    pub allow_cancel: bool,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserPromptResponseSubmission {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AskUserPromptDecision {
    pub status: AskUserPromptStatus,
    pub response: AskUserPromptResponseSubmission,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserPromptRecord {
    pub id: String,
    #[serde(rename = "conversation_id")]
    pub conversation_id: String,
    pub conversation_turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub kind: String,
    pub status: AskUserPromptStatus,
    pub prompt: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_prompt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn default_timeout_ms() -> u64 {
    ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT
}

fn default_allow_cancel() -> bool {
    true
}

fn default_source() -> String {
    "chatos".to_string()
}
