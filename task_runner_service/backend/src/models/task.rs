// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{TaskBuiltinMcpPromptMode, TaskMcpInitMode};
use chatos_mcp_runtime::BuiltinMcpPromptLocale;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::CreateRemoteServerRequest;

mod config;
mod record;
mod requests;

pub use self::config::*;
pub use self::record::*;
pub use self::requests::*;

pub const TASK_PROFILE_DEFAULT: &str = "default";
pub const TASK_PROFILE_CHATOS_PLAN: &str = "chatos_plan";

pub fn default_task_profile() -> String {
    TASK_PROFILE_DEFAULT.to_string()
}

pub fn normalize_task_profile(value: Option<&str>) -> Result<String, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(default_task_profile());
    };
    if value.eq_ignore_ascii_case(TASK_PROFILE_DEFAULT) {
        return Ok(TASK_PROFILE_DEFAULT.to_string());
    }
    if value.eq_ignore_ascii_case(TASK_PROFILE_CHATOS_PLAN) {
        return Ok(TASK_PROFILE_CHATOS_PLAN.to_string());
    }
    Err(format!("unknown task_profile: {value}"))
}

pub fn uses_task_runner_planning_agent(task_profile: &str, requires_execution: bool) -> bool {
    task_profile
        .trim()
        .eq_ignore_ascii_case(TASK_PROFILE_CHATOS_PLAN)
        && !requires_execution
}

pub fn task_runner_agent_key_for(
    task_profile: &str,
    requires_execution: bool,
) -> chatos_plugin_management_sdk::SystemAgentKey {
    if uses_task_runner_planning_agent(task_profile, requires_execution) {
        chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerPlanPhase
    } else {
        chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
    }
}

#[cfg(test)]
mod task_runner_agent_routing_tests {
    use super::*;
    use chatos_plugin_management_sdk::SystemAgentKey;

    #[test]
    fn only_non_executing_plan_tasks_use_the_planning_agent() {
        assert_eq!(
            task_runner_agent_key_for(TASK_PROFILE_CHATOS_PLAN, false),
            SystemAgentKey::TaskRunnerPlanPhase
        );
        assert_eq!(
            task_runner_agent_key_for(TASK_PROFILE_CHATOS_PLAN, true),
            SystemAgentKey::TaskRunnerRunPhase
        );
        assert_eq!(
            task_runner_agent_key_for(TASK_PROFILE_DEFAULT, false),
            SystemAgentKey::TaskRunnerRunPhase
        );
    }
}
