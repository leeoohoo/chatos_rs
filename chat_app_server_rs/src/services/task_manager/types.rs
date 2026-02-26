use serde::{Deserialize, Serialize};

use super::normalizer::{normalize_priority, normalize_status, normalize_tags, trimmed_non_empty};

pub const REVIEW_TIMEOUT_MS_DEFAULT: u64 = 86_400_000;
pub const REVIEW_TIMEOUT_ERR: &str = "review_timeout";
pub const REVIEW_NOT_FOUND_ERR: &str = "review_not_found";
pub const TASK_NOT_FOUND_ERR: &str = "task_not_found";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDraft {
    pub title: String,
    #[serde(default)]
    pub details: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub due_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskUpdatePatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub due_at: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub session_id: String,
    pub conversation_turn_id: String,
    pub title: String,
    pub details: String,
    pub priority: String,
    pub status: String,
    pub tags: Vec<String>,
    pub due_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreateReviewPayload {
    pub review_id: String,
    pub session_id: String,
    pub conversation_turn_id: String,
    pub draft_tasks: Vec<TaskDraft>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskReviewAction {
    Confirm,
    Cancel,
}

impl TaskReviewAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskReviewDecision {
    pub action: TaskReviewAction,
    pub tasks: Vec<TaskDraft>,
    pub reason: Option<String>,
}

impl TaskUpdatePatch {
    pub(super) fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.details.is_none()
            && self.priority.is_none()
            && self.status.is_none()
            && self.tags.is_none()
            && self.due_at.is_none()
    }

    pub(super) fn normalized(mut self) -> Result<Self, String> {
        if let Some(title) = self.title.take() {
            let title = title.trim().to_string();
            if title.is_empty() {
                return Err("task title is required".to_string());
            }
            self.title = Some(title);
        }

        if let Some(details) = self.details.take() {
            self.details = Some(details.trim().to_string());
        }

        if let Some(priority) = self.priority.take() {
            self.priority = Some(normalize_priority(priority.as_str()));
        }

        if let Some(status) = self.status.take() {
            self.status = Some(normalize_status(status.as_str()));
        }

        if let Some(tags) = self.tags.take() {
            self.tags = Some(normalize_tags(tags));
        }

        if let Some(due_at) = self.due_at.take() {
            let normalized = due_at
                .as_deref()
                .and_then(trimmed_non_empty)
                .map(|value| value.to_string());
            self.due_at = Some(normalized);
        }

        Ok(self)
    }
}

fn default_priority() -> String {
    "medium".to_string()
}

fn default_status() -> String {
    "todo".to_string()
}
