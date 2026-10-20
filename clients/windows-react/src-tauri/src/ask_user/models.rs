use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskUserPromptStatus {
    Pending,
    Ok,
    Canceled,
    Timeout,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskUserField {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub placeholder: Option<String>,
    pub default_value: String,
    pub is_required: bool,
    pub is_multiline: bool,
    pub is_secret: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskUserChoiceOption {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskUserChoice {
    pub allows_multiple: bool,
    pub options: Vec<AskUserChoiceOption>,
    pub default_selection: Vec<String>,
    pub minimum_selection_count: usize,
    pub maximum_selection_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskUserPrompt {
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub tool_call_id: Option<String>,
    pub kind: String,
    pub status: AskUserPromptStatus,
    pub title: String,
    pub message: String,
    pub allows_cancel: bool,
    pub timeout_milliseconds: Option<i64>,
    pub fields: Vec<AskUserField>,
    pub choice: Option<AskUserChoice>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

impl AskUserPrompt {
    pub fn with_status(mut self, status: AskUserPromptStatus, updated_at_unix_ms: i64) -> Self {
        self.status = status;
        self.updated_at_unix_ms = updated_at_unix_ms;
        self
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum AskUserSelectionInput {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskUserSubmissionInput {
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    #[serde(default)]
    pub selection: Option<AskUserSelectionInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserMutationInput {
    pub owner_user_id: String,
    pub conversation_id: String,
    pub prompt_id: String,
    #[serde(default)]
    pub submission: Option<AskUserSubmissionInput>,
}
