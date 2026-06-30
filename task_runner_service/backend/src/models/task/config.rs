use super::*;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Draft,
    Ready,
    Queued,
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
pub enum TaskProcessLogOperation {
    Append,
    Replace,
    Clear,
}

impl Default for TaskProcessLogOperation {
    fn default() -> Self {
        Self::Append
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
    #[serde(default)]
    pub external_mcp_config_ids: Vec<String>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
}

impl Default for TaskMcpConfig {
    fn default() -> Self {
        Self {
            enabled: task_mcp_enabled_default(),
            init_mode: TaskMcpInitMode::Full,
            builtin_prompt_mode: TaskBuiltinMcpPromptMode::Effective,
            builtin_prompt_locale: task_mcp_locale_default(),
            enabled_builtin_kinds: task_mcp_builtin_kinds_default(),
            workspace_dir: None,
            default_remote_server_id: None,
            external_mcp_config_ids: Vec::new(),
            skill_ids: Vec::new(),
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
        .filter(|kind| !matches!(kind, chatos_mcp_runtime::BuiltinMcpKind::ProjectManagement))
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskScheduleMode {
    Manual,
    Once,
    Interval,
    ContactAsync,
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
    #[serde(default)]
    pub cancel_reason: Option<String>,
    #[serde(default)]
    pub cancelled_at: Option<String>,
    #[serde(default)]
    pub cancelled_by_user_id: Option<String>,
    #[serde(default)]
    pub cancelled_by_username: Option<String>,
    #[serde(default)]
    pub cancelled_by_display_name: Option<String>,
    #[serde(default)]
    pub replacement_task_ids: Vec<String>,
    #[serde(default)]
    pub cancelled_because_task_id: Option<String>,
    #[serde(default)]
    pub cascade_root_task_id: Option<String>,
}
