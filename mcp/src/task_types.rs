// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

pub const TASK_NOT_FOUND_ERR: &str = "task_not_found";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskOutcomeItem {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub importance: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
}

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
    pub prerequisite_task_id: Option<String>,
    #[serde(default)]
    pub prerequisite_task_ids: Vec<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub outcome_summary: String,
    #[serde(default)]
    pub outcome_items: Vec<TaskOutcomeItem>,
    #[serde(default)]
    pub resume_hint: String,
    #[serde(default)]
    pub blocker_reason: String,
    #[serde(default)]
    pub blocker_needs: Vec<String>,
    #[serde(default)]
    pub blocker_kind: String,
    #[serde(default = "default_task_scope")]
    pub scope: String,
    #[serde(default = "default_required_for_parent_completion")]
    pub required_for_parent_completion: bool,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClosureDecision {
    pub task_id: String,
    pub closure_state: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub outcome_summary: String,
    #[serde(default)]
    pub outcome_items: Vec<TaskOutcomeItem>,
    #[serde(default)]
    pub resume_hint: String,
}

pub type TaskUpdatePatch = chatos_mcp_runtime::TaskUpdatePatch<TaskOutcomeItem>;

fn default_priority() -> String {
    "medium".to_string()
}

fn default_status() -> String {
    "todo".to_string()
}

fn default_task_scope() -> String {
    "run_checklist".to_string()
}

fn default_required_for_parent_completion() -> bool {
    true
}
