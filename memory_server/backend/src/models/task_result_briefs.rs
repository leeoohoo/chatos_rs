use serde::{Deserialize, Serialize};

use super::default_i64_0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResultBrief {
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
    #[serde(default = "default_i64_0")]
    pub agent_memory_summarized: i64,
    pub agent_memory_summarized_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertTaskResultBriefRequest {
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
}
