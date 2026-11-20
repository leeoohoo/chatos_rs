// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{memory::memory_plan, LocalAiStepPlanner, PreparedLocalAiStep};
use async_trait::async_trait;
use chatos_ai_runtime::{
    message_item, ComposeContextPolicy, ContextualTurnRunner, ModelRuntimeConfig, RuntimeTurnSpec,
};
use chatos_local_agent_protocol::{
    task_acceptance_report_for_run, task_acceptance_report_is_terminal, LocalAgentRunClaim,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::planner_support::{
    apply_run_thinking_level, completed_async_task_handoff, completed_task_outcome_report,
    durable_step_input, main_chat_scoped_tools, merge_instructions, merge_local_tools, tool_name,
    validate_unique_tool_names,
};
use crate::planner_tools::task_scoped_tools;

#[cfg(test)]
use crate::planner_support::requirement_survey_output_item;

pub const MAIN_CHAT_PROFILE_KEY: &str = "main_chat";
pub const TASK_EXECUTION_PROFILE_KEY: &str = "task_execution";

include!("planner/context.rs");
include!("planner/contracts.rs");
include!("planner/builder.rs");
include!("planner/execution.rs");

#[cfg(test)]
#[path = "planner_tests.rs"]
mod tests;

#[cfg(test)]
mod agent_direct_tests;
