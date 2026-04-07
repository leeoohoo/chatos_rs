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

fn default_queue_position() -> i64 {
    0
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContextAssetRef {
    pub asset_type: String,
    pub asset_id: String,
    pub display_name: Option<String>,
    pub source_type: Option<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionResultContract {
    #[serde(default = "default_true")]
    pub result_required: bool,
    pub preferred_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlanningSnapshot {
    #[serde(default)]
    pub contact_authorized_builtin_mcp_ids: Vec<String>,
    pub selected_model_config_id: Option<String>,
    pub source_user_goal_summary: Option<String>,
    pub source_constraints_summary: Option<String>,
    pub planned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactTask {
    pub id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub scope_key: String,
    pub project_root: Option<String>,
    pub remote_connection_id: Option<String>,
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
    #[serde(default = "default_queue_position")]
    pub queue_position: i64,
    #[serde(default = "default_pending_confirm")]
    pub status: String,
    pub confirm_note: Option<String>,
    pub execution_note: Option<String>,
    #[serde(default)]
    pub planned_builtin_mcp_ids: Vec<String>,
    #[serde(default)]
    pub planned_context_assets: Vec<TaskContextAssetRef>,
    pub execution_result_contract: Option<TaskExecutionResultContract>,
    pub planning_snapshot: Option<TaskPlanningSnapshot>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub confirmed_at: Option<String>,
    pub started_at: Option<String>,
    pub paused_at: Option<String>,
    pub pause_reason: Option<String>,
    pub last_checkpoint_summary: Option<String>,
    pub last_checkpoint_message_id: Option<String>,
    pub resume_note: Option<String>,
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
    pub control_request: Option<String>,
    pub control_requested_at: Option<String>,
    pub control_reason: Option<String>,
    pub resume_target_task_id: Option<String>,
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
    pub project_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub session_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub source_message_id: Option<String>,
    pub model_config_id: Option<String>,
    pub title: String,
    pub content: String,
    pub priority: Option<String>,
    pub confirm_note: Option<String>,
    pub execution_note: Option<String>,
    #[serde(default)]
    pub planned_builtin_mcp_ids: Vec<String>,
    #[serde(default)]
    pub planned_context_assets: Vec<TaskContextAssetRef>,
    pub execution_result_contract: Option<TaskExecutionResultContract>,
    pub planning_snapshot: Option<TaskPlanningSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub status: Option<String>,
    pub confirm_note: Option<String>,
    pub execution_note: Option<String>,
    pub project_root: Option<Option<String>>,
    pub remote_connection_id: Option<Option<String>>,
    pub planned_builtin_mcp_ids: Option<Vec<String>>,
    pub planned_context_assets: Option<Vec<TaskContextAssetRef>>,
    pub execution_result_contract: Option<TaskExecutionResultContract>,
    pub planning_snapshot: Option<TaskPlanningSnapshot>,
    pub model_config_id: Option<Option<String>>,
    pub queue_position: Option<i64>,
    pub pause_reason: Option<Option<String>>,
    pub last_checkpoint_summary: Option<Option<String>>,
    pub last_checkpoint_message_id: Option<Option<String>>,
    pub resume_note: Option<Option<String>>,
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
pub struct PauseTaskRequest {
    pub user_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopTaskRequest {
    pub user_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeTaskRequest {
    pub user_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckPauseTaskRequest {
    pub checkpoint_summary: Option<String>,
    pub checkpoint_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckStopTaskRequest {
    pub result_summary: Option<String>,
    pub result_message_id: Option<String>,
    pub last_error: Option<String>,
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
pub struct TaskResultBriefView {
    pub id: String,
    pub task_id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub task_title: String,
    pub task_status: String,
    pub result_summary: String,
    pub result_format: Option<String>,
    pub result_message_id: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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
