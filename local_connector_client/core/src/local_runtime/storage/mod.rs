// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod agent_prompts;
mod ask_user;
mod capabilities;
mod database;
mod events;
mod guidance;
mod mcp_manifests;
mod memory;
mod messages;
mod models;
mod project_management;
mod projects;
mod runtime_environment;
mod runtime_settings;
mod sessions;
mod subject_memory;
mod task_board;
mod task_runs;
mod turn_queries;
mod turns;

pub(crate) use agent_prompts::LocalAgentPromptRecord;
pub(crate) use ask_user::LocalAskUserPromptRecord;
pub(crate) use database::{database_path_for_state, LocalDatabase};
pub(crate) use models::{
    AppendLocalMessageInput, AppendLocalRuntimeEventInput, BeginLocalTurnInput,
    BeginLocalTurnResult, CompleteLocalTurnInput, CreateLocalMemorySummaryInput,
    CreateLocalSessionInput, LocalMemoryContext, LocalMemorySummaryRecord, LocalMessageRecord,
    LocalProjectRecord, LocalRuntimeDatabaseHealth, LocalRuntimeEventRecord,
    LocalRuntimeSettingsRecord, LocalSessionRecord, LocalSubjectMemoryRecord,
    LocalSubjectMemoryRollupPlan, LocalTurnRecord, LocalTurnSnapshot,
    SaveLocalRuntimeSettingsInput, SaveLocalSubjectMemoryRollupInput, UpsertLocalProjectInput,
};

#[cfg(test)]
mod tests;
