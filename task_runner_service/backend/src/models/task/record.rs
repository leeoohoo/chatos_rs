use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub objective: String,
    pub input_payload: Option<Value>,
    pub status: TaskStatus,
    pub priority: i32,
    pub tags: Vec<String>,
    pub default_model_config_id: Option<String>,
    pub memory_thread_id: String,
    pub tenant_id: String,
    pub subject_id: String,
    #[serde(default = "default_task_project_id")]
    pub project_id: String,
    #[serde(default = "default_task_profile")]
    pub task_profile: String,
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    pub result_summary: Option<String>,
    #[serde(default)]
    pub process_log: Option<String>,
    pub last_run_id: Option<String>,
    #[serde(default)]
    pub schedule: TaskScheduleConfig,
    #[serde(default)]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub source_run_id: Option<String>,
    #[serde(default)]
    pub source_session_id: Option<String>,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    #[serde(default)]
    pub source_user_message_id: Option<String>,
    #[serde(default)]
    pub prerequisite_task_ids: Vec<String>,
    #[serde(default)]
    pub task_tool_state: TaskToolState,
    pub mcp_config: TaskMcpConfig,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

fn default_task_project_id() -> String {
    crate::models::PUBLIC_PROJECT_ID.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPrerequisiteRecord {
    pub task_id: String,
    pub prerequisite_task_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDependencyGraph {
    pub task_id: String,
    pub prerequisites: Vec<TaskSummaryRecord>,
    pub transitive_prerequisites: Vec<TaskSummaryRecord>,
    pub blocked_by: Vec<TaskSummaryRecord>,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummaryRecord {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub default_model_config_id: Option<String>,
    #[serde(default = "default_task_project_id")]
    pub project_id: String,
    pub creator_user_id: Option<String>,
    pub creator_username: Option<String>,
    pub creator_display_name: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    pub last_run_id: Option<String>,
    pub updated_at: String,
}

impl From<&TaskRecord> for TaskSummaryRecord {
    fn from(value: &TaskRecord) -> Self {
        Self {
            id: value.id.clone(),
            title: value.title.clone(),
            status: value.status,
            default_model_config_id: value.default_model_config_id.clone(),
            project_id: value.project_id.clone(),
            creator_user_id: value.creator_user_id.clone(),
            creator_username: value.creator_username.clone(),
            creator_display_name: value.creator_display_name.clone(),
            owner_user_id: value.owner_user_id.clone(),
            owner_username: value.owner_username.clone(),
            owner_display_name: value.owner_display_name.clone(),
            last_run_id: value.last_run_id.clone(),
            updated_at: value.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIndexResponse {
    pub tasks: Vec<TaskSummaryRecord>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatsResponse {
    pub total: usize,
    pub scheduled: usize,
    pub follow_up: usize,
    pub draft: usize,
    pub ready: usize,
    pub queued: usize,
    pub running: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub blocked: usize,
    pub cancelled: usize,
    pub archived: usize,
}
