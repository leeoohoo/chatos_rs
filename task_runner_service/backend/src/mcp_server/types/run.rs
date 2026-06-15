use super::*;

#[derive(Debug, Default, Deserialize)]
pub(in crate::mcp_server) struct ListRunsArgs {
    #[serde(default)]
    pub(in crate::mcp_server) task_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) status: Option<TaskRunStatus>,
    #[serde(default)]
    pub(in crate::mcp_server) model_config_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct RunIdArgs {
    pub(in crate::mcp_server) run_id: String,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct StartTaskRunArgs {
    pub(in crate::mcp_server) task_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) model_config_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) prompt_override: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::mcp_server) struct GetTaskMemoryContextArgs {
    pub(in crate::mcp_server) task_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) include_recent_records: Option<bool>,
    #[serde(default)]
    pub(in crate::mcp_server) include_thread_summary: Option<bool>,
    #[serde(default)]
    pub(in crate::mcp_server) include_subject_memory: Option<bool>,
    #[serde(default)]
    pub(in crate::mcp_server) recent_record_limit: Option<usize>,
    #[serde(default)]
    pub(in crate::mcp_server) summary_limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::mcp_server) struct ListTaskMemoryRecordsArgs {
    pub(in crate::mcp_server) task_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) role: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) record_type: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) summary_status: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) limit: Option<i64>,
    #[serde(default)]
    pub(in crate::mcp_server) offset: Option<i64>,
    #[serde(default)]
    pub(in crate::mcp_server) order: Option<String>,
}
