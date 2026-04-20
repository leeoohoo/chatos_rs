use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const UI_PROMPT_TIMEOUT_MS_DEFAULT: u64 = 86_400_000;
pub const UI_PROMPT_TIMEOUT_ERR: &str = "ui_prompt_timeout";
pub const UI_PROMPT_NOT_FOUND_ERR: &str = "ui_prompt_not_found";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UiPromptStatus {
    Pending,
    Ok,
    Canceled,
    Timeout,
}

impl UiPromptStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ok => "ok",
            Self::Canceled => "canceled",
            Self::Timeout => "timeout",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "ok" => Some(Self::Ok),
            "canceled" | "cancelled" => Some(Self::Canceled),
            "timeout" => Some(Self::Timeout),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPromptPayload {
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
pub struct UiPromptResponseSubmission {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UiPromptDecision {
    pub status: UiPromptStatus,
    pub response: UiPromptResponseSubmission,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPromptRecord {
    pub id: String,
    #[serde(rename = "conversation_id")]
    pub conversation_id: String,
    pub conversation_turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub kind: String,
    pub status: UiPromptStatus,
    pub prompt: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn default_timeout_ms() -> u64 {
    UI_PROMPT_TIMEOUT_MS_DEFAULT
}

fn default_allow_cancel() -> bool {
    true
}
