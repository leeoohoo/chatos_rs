// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::ask_user_prompt_service::AskUserPromptService;
use crate::services::{ModelConfigService, RunService, TaskService};
pub(crate) use chatos_agent::{
    CHATOS_ASYNC_PLANNER_TOOL_PROFILE, PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE,
};

#[path = "mcp_server/access.rs"]
mod access;
#[path = "mcp_server/chatos_async_planner.rs"]
mod chatos_async_planner;
#[path = "mcp_server/context.rs"]
mod context;
#[path = "mcp_server/dispatch.rs"]
mod dispatch;
#[path = "mcp_server/entrypoints.rs"]
mod entrypoints;
#[path = "mcp_server/model_tools.rs"]
mod model_tools;
#[path = "mcp_server/prerequisite_creation.rs"]
mod prerequisite_creation;
#[path = "mcp_server/prompt_tools.rs"]
mod prompt_tools;
#[path = "mcp_server/run_tools.rs"]
mod run_tools;
#[path = "mcp_server/support.rs"]
mod support;
#[path = "mcp_server/task_tools.rs"]
mod task_tools;
#[cfg(test)]
#[path = "mcp_server/tests.rs"]
mod tests;
#[path = "mcp_server/types.rs"]
mod types;

pub use self::context::McpRequestContext;
use self::context::McpToolProfile;
use self::types::*;
pub use self::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

const TASK_RUNNER_MCP_ENDPOINT_PATH: &str = "/mcp";
const TASK_RUNNER_MCP_STDIO_COMMAND: &str = "cargo";
const TASK_RUNNER_MCP_STDIO_ARGS: &[&str] = &[
    "run",
    "-p",
    "task_runner_service_backend",
    "--bin",
    "task_runner_mcp_stdio",
];
#[derive(Clone)]
pub struct TaskRunnerMcpService {
    task_service: TaskService,
    model_config_service: ModelConfigService,
    run_service: RunService,
    ask_user_prompt_service: AskUserPromptService,
}

impl TaskRunnerMcpService {
    pub(crate) fn new(
        task_service: TaskService,
        model_config_service: ModelConfigService,
        run_service: RunService,
        ask_user_prompt_service: AskUserPromptService,
    ) -> Self {
        Self {
            task_service,
            model_config_service,
            run_service,
            ask_user_prompt_service,
        }
    }
}
