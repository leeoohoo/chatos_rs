use serde::Serialize;
use serde_json::Value;

use crate::models::TaskScheduleConfig;

use super::{
    normalized_optional, sanitize_task_list_filters, CurrentUser, TaskListFilters,
    TaskMcpConfig, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskService, TaskStatus,
    TaskToolState,
};

#[derive(Debug, Clone, Serialize)]
pub struct ChatosMessageTaskSummary {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub last_run_id: Option<String>,
    pub result_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatosMessageTaskDetail {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub objective: String,
    pub status: TaskStatus,
    pub priority: i32,
    pub tags: Vec<String>,
    pub default_model_config_id: Option<String>,
    pub creator_user_id: Option<String>,
    pub creator_username: Option<String>,
    pub creator_display_name: Option<String>,
    pub result_summary: Option<String>,
    pub process_log: Option<String>,
    pub last_run_id: Option<String>,
    pub schedule: TaskScheduleConfig,
    pub parent_task_id: Option<String>,
    pub source_run_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub prerequisite_task_ids: Vec<String>,
    pub task_tool_state: TaskToolState,
    pub mcp_config: TaskMcpConfig,
    pub input_payload: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatosMessageTaskRun {
    pub id: String,
    pub task_id: String,
    pub model_config_id: String,
    pub status: super::TaskRunStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub input_snapshot: Value,
    pub context_snapshot: Option<Value>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub usage: Option<Value>,
    pub report: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatosMessageTaskRunEvent {
    pub id: String,
    pub run_id: String,
    pub event_type: String,
    pub message: Option<String>,
    pub payload: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatosMessageRunDetail {
    pub task: ChatosMessageTaskDetail,
    pub run: ChatosMessageTaskRun,
    pub events: Vec<ChatosMessageTaskRunEvent>,
}

impl From<TaskRecord> for ChatosMessageTaskSummary {
    fn from(task: TaskRecord) -> Self {
        Self {
            id: task.id,
            title: task.title,
            description: task.description,
            status: task.status,
            last_run_id: task.last_run_id,
            result_summary: task.result_summary,
            created_at: task.created_at,
            updated_at: task.updated_at,
            source_session_id: task.source_session_id,
            source_user_message_id: task.source_user_message_id,
        }
    }
}

impl From<TaskRecord> for ChatosMessageTaskDetail {
    fn from(task: TaskRecord) -> Self {
        Self {
            id: task.id,
            title: task.title,
            description: task.description,
            objective: task.objective,
            status: task.status,
            priority: task.priority,
            tags: task.tags,
            default_model_config_id: task.default_model_config_id,
            creator_user_id: task.creator_user_id,
            creator_username: task.creator_username,
            creator_display_name: task.creator_display_name,
            result_summary: task.result_summary,
            process_log: task.process_log,
            last_run_id: task.last_run_id,
            schedule: task.schedule,
            parent_task_id: task.parent_task_id,
            source_run_id: task.source_run_id,
            source_session_id: task.source_session_id,
            source_turn_id: task.source_turn_id,
            source_user_message_id: task.source_user_message_id,
            prerequisite_task_ids: task.prerequisite_task_ids,
            task_tool_state: task.task_tool_state,
            mcp_config: task.mcp_config,
            input_payload: task.input_payload,
            created_at: task.created_at,
            updated_at: task.updated_at,
        }
    }
}

impl From<TaskRunRecord> for ChatosMessageTaskRun {
    fn from(run: TaskRunRecord) -> Self {
        Self {
            id: run.id,
            task_id: run.task_id,
            model_config_id: run.model_config_id,
            status: run.status,
            started_at: run.started_at,
            finished_at: run.finished_at,
            input_snapshot: run.input_snapshot,
            context_snapshot: run.context_snapshot,
            result_summary: run.result_summary,
            error_message: run.error_message,
            usage: run.usage,
            report: run.report,
            created_at: run.created_at,
            updated_at: run.updated_at,
        }
    }
}

impl From<TaskRunEventRecord> for ChatosMessageTaskRunEvent {
    fn from(event: TaskRunEventRecord) -> Self {
        Self {
            id: event.id,
            run_id: event.run_id,
            event_type: event.event_type,
            message: event.message,
            payload: event.payload,
            created_at: event.created_at,
        }
    }
}

fn normalize_source_id(value: &str) -> Option<String> {
    normalized_optional(Some(value.to_string()))
}

fn task_matches_source_user_message(task: &TaskRecord, source_user_message_id: &str) -> bool {
    task.source_user_message_id.as_deref() == Some(source_user_message_id)
}

fn task_matches_chatos_message_source(
    task: &TaskRecord,
    source_session_id: &str,
    source_user_message_id: &str,
) -> bool {
    task.source_session_id.as_deref() == Some(source_session_id)
        && task.source_user_message_id.as_deref() == Some(source_user_message_id)
}

impl TaskService {
    pub async fn list_tasks_for_source_user_message(
        &self,
        source_user_message_id: &str,
        creator: Option<&CurrentUser>,
    ) -> Result<Vec<TaskRecord>, String> {
        let Some(source_user_message_id) = normalize_source_id(source_user_message_id) else {
            return Ok(Vec::new());
        };
        let filters = sanitize_task_list_filters(TaskListFilters {
            creator_user_id: creator.map(|user| user.id.clone()),
            ..TaskListFilters::default()
        });
        let tasks = self.store.list_tasks_filtered(&filters).await?;
        let tasks = tasks
            .into_iter()
            .filter(|task| task_matches_source_user_message(task, source_user_message_id.as_str()))
            .collect::<Vec<_>>();
        self.hydrate_tasks_prerequisites(tasks).await
    }

    pub async fn list_tasks_for_chatos_message(
        &self,
        source_session_id: &str,
        source_user_message_id: &str,
    ) -> Result<Vec<TaskRecord>, String> {
        let Some(source_session_id) = normalize_source_id(source_session_id) else {
            return Ok(Vec::new());
        };
        let Some(source_user_message_id) = normalize_source_id(source_user_message_id) else {
            return Ok(Vec::new());
        };
        let mut tasks = self
            .store
            .list_tasks_filtered(&TaskListFilters::default())
            .await?
            .into_iter()
            .filter(|task| {
                task_matches_chatos_message_source(
                    task,
                    source_session_id.as_str(),
                    source_user_message_id.as_str(),
                )
            })
            .collect::<Vec<_>>();
        tasks.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        self.hydrate_tasks_prerequisites(tasks).await
    }

    pub async fn list_message_task_summaries_for_chatos_message(
        &self,
        source_session_id: &str,
        source_user_message_id: &str,
    ) -> Result<Vec<ChatosMessageTaskSummary>, String> {
        Ok(self
            .list_tasks_for_chatos_message(source_session_id, source_user_message_id)
            .await?
            .into_iter()
            .map(ChatosMessageTaskSummary::from)
            .collect())
    }

    pub async fn get_task_for_chatos_message(
        &self,
        task_id: &str,
        source_session_id: &str,
        source_user_message_id: &str,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(task) = self.get_task(task_id).await? else {
            return Ok(None);
        };
        if task_matches_chatos_message_source(&task, source_session_id, source_user_message_id) {
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    pub async fn get_message_task_detail_for_chatos_message(
        &self,
        task_id: &str,
        source_session_id: &str,
        source_user_message_id: &str,
    ) -> Result<Option<ChatosMessageTaskDetail>, String> {
        Ok(self
            .get_task_for_chatos_message(task_id, source_session_id, source_user_message_id)
            .await?
            .map(ChatosMessageTaskDetail::from))
    }
}
