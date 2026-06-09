use chatos_ai_runtime::{ModelRuntimeConfig, TaskBuiltinMcpPromptMode, TaskMcpInitMode};
use chatos_builtin_tools::{UiPromptPayload, UiPromptResponseSubmission};
use chatos_mcp_runtime::{
    configurable_builtin_kinds, BuiltinMcpPromptBuildResult, BuiltinMcpPromptLocale,
};
use chrono::Utc;
use memory_engine_sdk::{
    ComposeContextResponse, EngineRecord, EngineThread, RunThreadRepairSummaryResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Draft,
    Ready,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    Archived,
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Draft
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
}

impl Default for TaskRunStatus {
    fn default() -> Self {
        Self::Queued
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiPromptStatus {
    Pending,
    Submitted,
    Cancelled,
    TimedOut,
    Failed,
}

impl Default for UiPromptStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMcpConfig {
    #[serde(default = "task_mcp_enabled_default")]
    pub enabled: bool,
    #[serde(default)]
    pub init_mode: TaskMcpInitMode,
    #[serde(default)]
    pub builtin_prompt_mode: TaskBuiltinMcpPromptMode,
    #[serde(default = "task_mcp_locale_default")]
    pub builtin_prompt_locale: String,
    #[serde(default = "task_mcp_builtin_kinds_default")]
    pub enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    pub workspace_dir: Option<String>,
    #[serde(default)]
    pub default_remote_server_id: Option<String>,
}

impl Default for TaskMcpConfig {
    fn default() -> Self {
        Self {
            enabled: task_mcp_enabled_default(),
            init_mode: TaskMcpInitMode::BuiltinOnly,
            builtin_prompt_mode: TaskBuiltinMcpPromptMode::Effective,
            builtin_prompt_locale: task_mcp_locale_default(),
            enabled_builtin_kinds: task_mcp_builtin_kinds_default(),
            workspace_dir: None,
            default_remote_server_id: None,
        }
    }
}

impl TaskMcpConfig {
    pub fn locale(&self) -> BuiltinMcpPromptLocale {
        BuiltinMcpPromptLocale::from_key(Some(&self.builtin_prompt_locale))
    }
}

fn task_mcp_enabled_default() -> bool {
    true
}

fn task_mcp_locale_default() -> String {
    BuiltinMcpPromptLocale::DEFAULT_KEY.to_string()
}

fn task_mcp_builtin_kinds_default() -> Vec<String> {
    configurable_builtin_kinds()
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskScheduleMode {
    Manual,
    Once,
    Interval,
}

impl Default for TaskScheduleMode {
    fn default() -> Self {
        Self::Manual
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskScheduleConfig {
    #[serde(default)]
    pub mode: TaskScheduleMode,
    #[serde(default)]
    pub run_at: Option<String>,
    #[serde(default)]
    pub interval_seconds: Option<i64>,
    #[serde(default)]
    pub next_run_at: Option<String>,
    #[serde(default)]
    pub last_scheduled_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskToolOutcomeItem {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub importance: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskToolState {
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub outcome_items: Vec<TaskToolOutcomeItem>,
    #[serde(default)]
    pub resume_hint: Option<String>,
    #[serde(default)]
    pub blocker_reason: Option<String>,
    #[serde(default)]
    pub blocker_needs: Vec<String>,
    #[serde(default)]
    pub blocker_kind: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub last_outcome_at: Option<String>,
}

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
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    pub result_summary: Option<String>,
    pub last_run_id: Option<String>,
    #[serde(default)]
    pub schedule: TaskScheduleConfig,
    #[serde(default)]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub source_run_id: Option<String>,
    #[serde(default)]
    pub task_tool_state: TaskToolState,
    pub mcp_config: TaskMcpConfig,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
}

impl From<&UserRecord> for AuthUser {
    fn from(value: &UserRecord) -> Self {
        Self {
            id: value.id.clone(),
            username: value.username.clone(),
            display_name: value.display_name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummaryRecord {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

impl From<&UserRecord> for UserSummaryRecord {
    fn from(value: &UserRecord) -> Self {
        Self {
            id: value.id.clone(),
            username: value.username.clone(),
            display_name: value.display_name.clone(),
            enabled: value.enabled,
            created_at: value.created_at.clone(),
            updated_at: value.updated_at.clone(),
            last_login_at: value.last_login_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigRecord {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub supports_responses: bool,
    pub instructions: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: bool,
    pub request_body_limit_bytes: Option<usize>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl ModelConfigRecord {
    pub fn to_runtime_config(&self, default_request_cwd: Option<String>) -> ModelRuntimeConfig {
        ModelRuntimeConfig::openai_compatible(
            self.base_url.clone(),
            self.api_key.clone(),
            self.model.clone(),
            self.provider.clone(),
        )
        .with_responses_support(self.supports_responses)
        .with_instructions(self.instructions.clone())
        .with_temperature(self.temperature)
        .with_max_output_tokens(self.max_output_tokens)
        .with_thinking_level(self.thinking_level.clone())
        .with_request_cwd(self.request_cwd.clone().or(default_request_cwd))
        .with_prompt_cache_retention(self.include_prompt_cache_retention)
        .with_request_body_limit_bytes(self.request_body_limit_bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteServerRecord {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: String,
    pub enabled: bool,
    pub last_tested_at: Option<String>,
    pub last_test_status: Option<String>,
    pub last_test_message: Option<String>,
    pub last_active_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRemoteServerRequest {
    pub name: String,
    pub host: String,
    pub port: Option<i64>,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRemoteServerRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub username: Option<String>,
    pub auth_type: Option<String>,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestRemoteServerRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub username: Option<String>,
    pub auth_type: Option<String>,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteServerTestResponse {
    pub ok: bool,
    pub server_id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub remote_host: Option<String>,
    pub error: Option<String>,
    pub tested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunRecord {
    pub id: String,
    pub task_id: String,
    pub model_config_id: String,
    pub memory_thread_id: String,
    pub status: TaskRunStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub input_snapshot: Value,
    pub context_snapshot: Option<Value>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub usage: Option<Value>,
    pub report: Option<Value>,
    pub cancel_requested: bool,
    pub summary_job_run_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunEventRecord {
    pub id: String,
    pub run_id: String,
    pub event_type: String,
    pub message: Option<String>,
    pub payload: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPromptRecord {
    pub id: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    pub conversation_id: String,
    pub conversation_turn_id: String,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub message: String,
    #[serde(default = "default_true")]
    pub allow_cancel: bool,
    pub timeout_ms: u64,
    pub payload: Value,
    #[serde(default)]
    pub response: Option<UiPromptResponseSubmission>,
    pub status: UiPromptStatus,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
}

impl UiPromptRecord {
    pub fn from_payload(
        payload: UiPromptPayload,
        task_id: Option<String>,
        run_id: Option<String>,
        created_at: String,
        expires_at: Option<String>,
    ) -> Self {
        Self {
            id: payload.prompt_id,
            task_id,
            run_id,
            conversation_id: payload.conversation_id,
            conversation_turn_id: payload.conversation_turn_id,
            tool_call_id: payload.tool_call_id,
            kind: payload.kind,
            title: payload.title,
            message: payload.message,
            allow_cancel: payload.allow_cancel,
            timeout_ms: payload.timeout_ms,
            payload: payload.payload,
            response: None,
            status: UiPromptStatus::Pending,
            created_at: created_at.clone(),
            updated_at: created_at,
            expires_at,
        }
    }
}

impl TaskRunEventRecord {
    pub fn new(
        run_id: impl Into<String>,
        event_type: impl Into<String>,
        message: Option<String>,
        payload: Option<Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            run_id: run_id.into(),
            event_type: event_type.into(),
            message,
            payload,
            created_at: now_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub objective: String,
    pub input_payload: Option<Value>,
    pub status: Option<TaskStatus>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub default_model_config_id: Option<String>,
    pub tenant_id: Option<String>,
    pub subject_id: Option<String>,
    pub schedule: Option<TaskScheduleConfig>,
    pub mcp_config: Option<TaskMcpConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    pub user: AuthUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: Option<String>,
    pub password: String,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub password: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub objective: Option<String>,
    pub input_payload: Option<Value>,
    pub status: Option<TaskStatus>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub default_model_config_id: Option<String>,
    pub schedule: Option<TaskScheduleConfig>,
    pub mcp_config: Option<TaskMcpConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTaskMcpRequest {
    pub enabled: Option<bool>,
    pub init_mode: Option<TaskMcpInitMode>,
    pub builtin_prompt_mode: Option<TaskBuiltinMcpPromptMode>,
    pub builtin_prompt_locale: Option<String>,
    pub enabled_builtin_kinds: Option<Vec<String>>,
    pub workspace_dir: Option<String>,
    pub default_remote_server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModelConfigRequest {
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub supports_responses: Option<bool>,
    pub instructions: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: Option<bool>,
    pub request_body_limit_bytes: Option<usize>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateModelConfigRequest {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub supports_responses: Option<bool>,
    pub instructions: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: Option<bool>,
    pub request_body_limit_bytes: Option<usize>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreviewModelCatalogRequest {
    pub provider: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelRecord {
    pub id: String,
    pub owned_by: Option<String>,
    pub context_length: Option<i64>,
    pub supports_images: bool,
    pub supports_video: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
    pub raw: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalogResponse {
    pub provider_config_id: Option<String>,
    pub provider: String,
    pub base_url: String,
    pub source: String,
    pub fetched_at: Option<String>,
    pub models: Vec<ProviderModelRecord>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteServerSummaryRecord {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub enabled: bool,
    pub updated_at: String,
    pub last_tested_at: Option<String>,
    pub last_test_status: Option<String>,
}

impl From<&RemoteServerRecord> for RemoteServerSummaryRecord {
    fn from(value: &RemoteServerRecord) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            host: value.host.clone(),
            port: value.port,
            username: value.username.clone(),
            auth_type: value.auth_type.clone(),
            enabled: value.enabled,
            updated_at: value.updated_at.clone(),
            last_tested_at: value.last_tested_at.clone(),
            last_test_status: value.last_test_status.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestModelConfigRequest {
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigTestResponse {
    pub ok: bool,
    pub model_config_id: String,
    pub provider: String,
    pub model: String,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
    pub error: Option<String>,
    pub tested_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskListFilters {
    pub status: Option<TaskStatus>,
    pub keyword: Option<String>,
    pub tag: Option<String>,
    pub model_config_id: Option<String>,
    pub scheduled_only: Option<bool>,
    pub parent_task_id: Option<String>,
    pub source_run_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunListFilters {
    pub task_id: Option<String>,
    pub status: Option<TaskRunStatus>,
    pub model_config_id: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptListFilters {
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub status: Option<UiPromptStatus>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummaryRecord {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub default_model_config_id: Option<String>,
    pub creator_user_id: Option<String>,
    pub creator_username: Option<String>,
    pub creator_display_name: Option<String>,
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
            creator_user_id: value.creator_user_id.clone(),
            creator_username: value.creator_username.clone(),
            creator_display_name: value.creator_display_name.clone(),
            last_run_id: value.last_run_id.clone(),
            updated_at: value.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummaryRecord {
    pub id: String,
    pub task_id: String,
    pub status: TaskRunStatus,
    pub model_config_id: String,
    pub updated_at: String,
}

impl From<&TaskRunRecord> for RunSummaryRecord {
    fn from(value: &TaskRunRecord) -> Self {
        Self {
            id: value.id.clone(),
            task_id: value.task_id.clone(),
            status: value.status,
            model_config_id: value.model_config_id.clone(),
            updated_at: value.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigUsageRecord {
    pub model_config_id: String,
    pub task_count: usize,
    pub run_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPromptTaskCountRecord {
    pub task_id: String,
    pub count: usize,
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
    pub running: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub blocked: usize,
    pub cancelled: usize,
    pub archived: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartTaskRunRequest {
    pub model_config_id: Option<String>,
    pub prompt_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTaskStatusUpdateRequest {
    pub task_ids: Vec<String>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTaskDeleteRequest {
    pub task_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchTaskRunRequest {
    pub task_ids: Vec<String>,
    pub model_config_id: Option<String>,
    pub prompt_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTaskOperationItem {
    pub task_id: String,
    pub ok: bool,
    pub message: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTaskOperationResponse {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<BatchTaskOperationItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskMemoryContextOptions {
    pub include_recent_records: Option<bool>,
    pub include_thread_summary: Option<bool>,
    pub include_subject_memory: Option<bool>,
    pub recent_record_limit: Option<usize>,
    pub summary_limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskMemoryRecordsOptions {
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub order: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemoryContextResponse {
    pub task_id: String,
    pub memory_thread_id: String,
    pub tenant_id: String,
    pub subject_id: String,
    pub thread: Option<EngineThread>,
    pub context: Option<ComposeContextResponse>,
    pub total_record_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemoryRecordsResponse {
    pub task_id: String,
    pub memory_thread_id: String,
    pub tenant_id: String,
    pub subject_id: String,
    pub thread: Option<EngineThread>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub order: String,
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
    pub has_more: bool,
    pub items: Vec<EngineRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemorySummaryResponse {
    pub task_id: String,
    pub memory_thread_id: String,
    pub tenant_id: String,
    pub requested_at: String,
    pub result: RunThreadRepairSummaryResponse,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubmitUiPromptRequest {
    pub values: Option<Value>,
    pub selection: Option<Value>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CancelUiPromptRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpUnavailableTool {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCatalogEntry {
    pub kind: String,
    pub server_name: String,
    pub config_id: Option<String>,
    pub command: Option<String>,
    pub implemented: bool,
    pub runtime_default: bool,
    pub default_allow_writes: bool,
    pub available_tool_names: Vec<String>,
    pub unavailable_tools: Vec<McpUnavailableTool>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub server_name: String,
    pub transports: Vec<String>,
    #[serde(default)]
    pub http_endpoint_path: Option<String>,
    #[serde(default)]
    pub stdio_command: Option<String>,
    #[serde(default)]
    pub stdio_args: Vec<String>,
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpPromptPreviewRequest {
    pub enabled: Option<bool>,
    pub init_mode: Option<TaskMcpInitMode>,
    pub builtin_prompt_mode: Option<TaskBuiltinMcpPromptMode>,
    pub builtin_prompt_locale: Option<String>,
    pub enabled_builtin_kinds: Option<Vec<String>>,
    pub workspace_dir: Option<String>,
    pub default_remote_server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptPreviewResponse {
    pub enabled: bool,
    pub init_mode: TaskMcpInitMode,
    pub builtin_prompt_mode: TaskBuiltinMcpPromptMode,
    pub builtin_prompt_locale: String,
    pub selected_builtin_kinds: Vec<String>,
    pub build: BuiltinMcpPromptBuildResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub now: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfigResponse {
    pub host: String,
    pub port: u16,
    pub store_mode: String,
    pub database_url: String,
    pub memory_engine_base_url: Option<String>,
    pub memory_engine_source_id: String,
    pub memory_engine_configured: bool,
    pub default_tenant_id: String,
    pub default_subject_id: String,
    pub default_workspace_dir: String,
    pub memory_timeout_ms: u64,
    pub execution_timeout_ms: u64,
    pub scheduler_poll_interval_ms: u64,
    pub auto_memory_summary: bool,
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn default_true() -> bool {
    true
}
