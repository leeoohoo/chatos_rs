use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalTaskRecord};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageTaskGraphInput {
    pub owner_user_id: String,
    pub conversation_id: String,
    pub turn_id: String,
    pub user_message_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInspectorInput {
    #[serde(flatten)]
    pub lookup: MessageTaskGraphInput,
    pub task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMutationInput {
    pub owner_user_id: String,
    pub task_id: String,
    pub expected_version: u64,
    #[serde(default)]
    pub instruction: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInspectorDetail {
    pub task: TaskInspectorTask,
    pub run: Option<RunInspectorRun>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInspectorTask {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub version: u64,
    pub description: Option<String>,
    pub objective: Option<String>,
    pub result_summary: Option<String>,
    pub process_entries: Vec<RunInspectorProcessEntry>,
    pub last_run_id: Option<String>,
    pub model_config_ref: String,
    pub thinking_level: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInspectorProcessEntry {
    pub id: String,
    pub heading: Option<String>,
    pub content: String,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInspectorDetail {
    pub run: RunInspectorRun,
    pub events: Vec<RunInspectorEvent>,
    pub events_has_more: bool,
    pub next_offset: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInspectorRun {
    pub run_id: String,
    pub task_id: String,
    pub status: String,
    pub iteration: u32,
    pub max_iterations: u32,
    pub result_summary: Option<String>,
    pub report_content: Option<String>,
    pub error_message: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub finished_at_unix_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInspectorEvent {
    pub cursor: i64,
    pub event_id: String,
    pub event_type: String,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMutationReceipt {
    pub task_id: String,
    pub status: String,
    pub active_run_id: Option<String>,
    pub version: u64,
}

impl TaskMutationReceipt {
    pub fn from_task(task: LocalTaskRecord) -> Self {
        Self {
            task_id: task.task_id,
            status: task.status.as_str().to_owned(),
            active_run_id: task.active_run_id,
            version: task.version,
        }
    }
}

impl RunInspectorRun {
    pub fn from_record(run: &LocalAgentRunRecord) -> Self {
        let outcome = run.terminal_outcome.as_ref();
        let model_output = outcome.and_then(super::projection::model_output);
        let report_content = outcome
            .and_then(|value| super::projection::string_field(value, &["report"]))
            .or_else(|| model_output.clone());
        let error_message = outcome.and_then(super::projection::execution_error);
        let result_summary = model_output;
        let terminal = matches!(
            run.status.to_string().as_str(),
            "succeeded" | "failed" | "cancelled" | "needs_review"
        );
        Self {
            run_id: run.run_id.clone(),
            task_id: run.owner_entity_id.clone(),
            status: run.status.to_string(),
            iteration: run.iteration,
            max_iterations: run.max_iterations,
            result_summary,
            report_content,
            error_message,
            created_at_unix_ms: run.created_at_unix_ms,
            updated_at_unix_ms: run.updated_at_unix_ms,
            finished_at_unix_ms: terminal.then_some(run.updated_at_unix_ms),
        }
    }
}
