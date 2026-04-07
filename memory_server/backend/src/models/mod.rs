use serde::{Deserialize, Deserializer};

fn default_active() -> String {
    "active".to_string()
}

fn default_pending() -> String {
    "pending".to_string()
}

fn default_i64_0() -> i64 {
    0
}

fn default_i64_1() -> i64 {
    1
}

fn default_keep_raw_level0_count() -> i64 {
    5
}

fn default_agent_memory_max_level() -> i64 {
    4
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn deserialize_vec_or_default<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<Vec<T>>::deserialize(deserializer).map(|value| value.unwrap_or_default())
}

fn deserialize_bool_true<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<bool>::deserialize(deserializer).map(|value| value.unwrap_or(true))
}

fn deserialize_string_active<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(|value| value.unwrap_or_else(default_active))
}

mod agents;
mod ai_models;
mod compose;
mod job_configs;
mod memories;
mod messages;
mod sessions;
mod summaries;
mod task_execution;
mod task_result_briefs;
mod turn_runtime_snapshots;

pub use self::agents::{
    CreateMemoryAgentRequest, MemoryAgent, MemoryAgentRuntimeCommandSummary,
    MemoryAgentRuntimeContext, MemoryAgentRuntimePluginSummary, MemoryAgentRuntimeSkillSummary,
    MemoryAgentSkill, MemorySkill, MemorySkillPlugin, MemorySkillPluginCommand,
    UpdateMemoryAgentRequest,
};
pub use self::ai_models::{AiModelConfig, UpsertAiModelConfigRequest};
pub use self::compose::{ComposeContextMeta, ComposeContextRequest, ComposeContextResponse};
pub use self::job_configs::{
    AgentMemoryJobConfig, JobRun, SummaryJobConfig, SummaryRollupJobConfig,
    TaskExecutionRollupJobConfig, TaskExecutionSummaryJobConfig, UpsertAgentMemoryJobConfigRequest,
    UpsertSummaryJobConfigRequest, UpsertSummaryRollupJobConfigRequest,
    UpsertTaskExecutionRollupJobConfigRequest, UpsertTaskExecutionSummaryJobConfigRequest,
    DEFAULT_SUMMARY_PROMPT_TEMPLATE, DEFAULT_TASK_EXECUTION_ROLLUP_PROMPT_TEMPLATE,
    DEFAULT_TASK_EXECUTION_SUMMARY_PROMPT_TEMPLATE,
};
pub use self::memories::{AgentRecall, ProjectMemory};
pub use self::messages::{BatchCreateMessagesRequest, CreateMessageRequest, Message};
pub use self::sessions::{
    Contact, CreateContactRequest, CreateSessionRequest, MemoryProject, MemoryProjectAgentLink,
    Session, UpdateSessionRequest,
};
pub use self::summaries::{CreateSummaryInput, SessionSummary};
pub use self::task_execution::{
    CreateTaskExecutionMessageRequest, CreateTaskExecutionSummaryInput,
    TaskExecutionComposeRequest, TaskExecutionComposeResponse, TaskExecutionMessage,
    TaskExecutionScope, TaskExecutionSummary,
};
pub use self::task_result_briefs::{TaskResultBrief, UpsertTaskResultBriefRequest};
pub use self::turn_runtime_snapshots::{
    SyncTurnRuntimeSnapshotRequest, TurnRuntimeSnapshot, TurnRuntimeSnapshotLookupResponse,
};
