use serde::{Deserialize, Serialize};

fn default_pending_confirm() -> String {
    "pending_confirm".to_string()
}

fn default_priority_medium() -> String {
    "medium".to_string()
}

fn default_priority_rank() -> i32 {
    20
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub user_id: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactTask {
    pub id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub scope_key: String,
    pub session_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub source_message_id: Option<String>,
    pub model_config_id: Option<String>,
    pub title: String,
    pub content: String,
    #[serde(default = "default_priority_medium")]
    pub priority: String,
    #[serde(default = "default_priority_rank")]
    pub priority_rank: i32,
    #[serde(default = "default_pending_confirm")]
    pub status: String,
    pub confirm_note: Option<String>,
    pub execution_note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub confirmed_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub last_error: Option<String>,
    pub result_summary: Option<String>,
    pub result_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactTaskScopeRuntime {
    pub scope_key: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub running_task_id: Option<String>,
    pub last_all_done_ack_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub user_id: Option<String>,
    pub contact_agent_id: String,
    pub project_id: String,
    pub session_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub source_message_id: Option<String>,
    pub model_config_id: Option<String>,
    pub title: String,
    pub content: String,
    pub priority: Option<String>,
    pub confirm_note: Option<String>,
    pub execution_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub status: Option<String>,
    pub confirm_note: Option<String>,
    pub execution_note: Option<String>,
    pub model_config_id: Option<Option<String>>,
    pub result_summary: Option<Option<String>>,
    pub result_message_id: Option<Option<String>>,
    pub last_error: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmTaskRequest {
    pub user_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerRequest {
    pub user_id: Option<String>,
    pub contact_agent_id: String,
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerDecision {
    pub decision: String,
    pub task: Option<ContactTask>,
    pub scope_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionScopeView {
    pub scope_key: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub latest_session_id: Option<String>,
    pub latest_task_id: Option<String>,
    pub latest_task_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionMessageView {
    pub id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub scope_key: String,
    pub task_id: Option<String>,
    pub source_session_id: Option<String>,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<serde_json::Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckAllDoneRequest {
    pub user_id: Option<String>,
    pub contact_agent_id: String,
    pub project_id: String,
    pub ack_at: Option<String>,
}

pub fn scope_key(user_id: &str, contact_agent_id: &str, project_id: &str) -> String {
    format!(
        "{}::{}::{}",
        user_id.trim(),
        contact_agent_id.trim(),
        project_id.trim()
    )
}

pub fn normalize_priority(input: Option<&str>) -> (String, i32) {
    match input
        .unwrap_or("medium")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "high" => ("high".to_string(), 10),
        "low" => ("low".to_string(), 30),
        _ => ("medium".to_string(), 20),
    }
}
