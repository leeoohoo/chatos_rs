use super::*;

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
    pub source_turn_id: Option<String>,
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
    pub default_model_config: Option<ChatosMessageModelConfigSummary>,
    pub creator_user_id: Option<String>,
    pub creator_username: Option<String>,
    pub creator_display_name: Option<String>,
    pub result_summary: Option<String>,
    pub process_log: Option<String>,
    pub last_run_id: Option<String>,
    pub last_run: Option<ChatosMessageTaskRunSummary>,
    pub schedule: TaskScheduleConfig,
    pub parent_task_id: Option<String>,
    pub parent_task: Option<TaskSummaryRecord>,
    pub source_run_id: Option<String>,
    pub source_run: Option<ChatosMessageTaskRunSummary>,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub prerequisite_task_ids: Vec<String>,
    pub prerequisite_tasks: Vec<TaskSummaryRecord>,
    pub task_tool_state: TaskToolState,
    pub mcp_config: TaskMcpConfig,
    pub input_payload: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatosMessageModelConfigSummary {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub usage_scenario: Option<String>,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatosMessageTaskRunSummary {
    pub id: String,
    pub task_id: String,
    pub model_config_id: String,
    pub status: super::TaskRunStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
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
    pub model_config: Option<ChatosMessageModelConfigSummary>,
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
            source_turn_id: task.source_turn_id,
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
            default_model_config: None,
            creator_user_id: task.creator_user_id,
            creator_username: task.creator_username,
            creator_display_name: task.creator_display_name,
            result_summary: task.result_summary,
            process_log: task.process_log,
            last_run_id: task.last_run_id,
            last_run: None,
            schedule: task.schedule,
            parent_task_id: task.parent_task_id,
            parent_task: None,
            source_run_id: task.source_run_id,
            source_run: None,
            source_session_id: task.source_session_id,
            source_turn_id: task.source_turn_id,
            source_user_message_id: task.source_user_message_id,
            prerequisite_task_ids: task.prerequisite_task_ids,
            prerequisite_tasks: Vec::new(),
            task_tool_state: task.task_tool_state,
            mcp_config: task.mcp_config,
            input_payload: task.input_payload,
            created_at: task.created_at,
            updated_at: task.updated_at,
        }
    }
}

impl ChatosMessageTaskDetail {
    pub fn from_parts(
        task: TaskRecord,
        default_model_config: Option<ChatosMessageModelConfigSummary>,
        last_run: Option<ChatosMessageTaskRunSummary>,
        parent_task: Option<TaskSummaryRecord>,
        source_run: Option<ChatosMessageTaskRunSummary>,
        prerequisite_tasks: Vec<TaskSummaryRecord>,
    ) -> Self {
        Self {
            default_model_config,
            last_run,
            parent_task,
            source_run,
            prerequisite_tasks,
            ..Self::from(task)
        }
    }
}

impl From<ModelConfigRecord> for ChatosMessageModelConfigSummary {
    fn from(model: ModelConfigRecord) -> Self {
        Self {
            id: model.id,
            name: model.name,
            provider: model.provider,
            model: model.model,
            usage_scenario: model.usage_scenario,
            enabled: model.enabled,
            updated_at: model.updated_at,
        }
    }
}

impl From<TaskRunRecord> for ChatosMessageTaskRunSummary {
    fn from(run: TaskRunRecord) -> Self {
        Self {
            id: run.id,
            task_id: run.task_id,
            model_config_id: run.model_config_id,
            status: run.status,
            started_at: run.started_at,
            finished_at: run.finished_at,
            result_summary: run.result_summary,
            error_message: run.error_message,
            created_at: run.created_at,
            updated_at: run.updated_at,
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
