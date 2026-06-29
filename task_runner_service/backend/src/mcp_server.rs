use crate::ask_user_prompt_service::AskUserPromptService;
use crate::services::{
    ExternalMcpConfigService, McpCatalogService, ModelConfigService, RunService, SkillService,
    TaskService,
};

mod access;
mod chatos_async_planner;
mod context;
mod dispatch;
mod entrypoints;
mod model_tools;
mod prerequisite_creation;
mod prompt_tools;
mod run_tools;
mod support;
mod task_tools;
#[cfg(test)]
mod tests;
mod types;

pub use self::context::McpRequestContext;
use self::context::McpToolProfile;
use self::types::*;
pub use self::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

const TASK_RUNNER_MCP_SERVER_NAME: &str = "task_runner_service";
const TASK_RUNNER_MCP_ENDPOINT_PATH: &str = "/mcp";
const TASK_RUNNER_MCP_STDIO_COMMAND: &str = "cargo";
const TASK_RUNNER_MCP_STDIO_ARGS: &[&str] = &[
    "run",
    "-p",
    "task_runner_service_backend",
    "--bin",
    "task_runner_mcp_stdio",
];
const CHATOS_ASYNC_PLANNER_TOOL_PROFILE: &str = "chatos_async_planner";

#[derive(Clone)]
pub struct TaskRunnerMcpService {
    task_service: TaskService,
    model_config_service: ModelConfigService,
    external_mcp_config_service: ExternalMcpConfigService,
    skill_service: SkillService,
    run_service: RunService,
    ask_user_prompt_service: AskUserPromptService,
    mcp_catalog_service: McpCatalogService,
}

impl TaskRunnerMcpService {
    pub(crate) fn new(
        task_service: TaskService,
        model_config_service: ModelConfigService,
        external_mcp_config_service: ExternalMcpConfigService,
        skill_service: SkillService,
        run_service: RunService,
        ask_user_prompt_service: AskUserPromptService,
        mcp_catalog_service: McpCatalogService,
    ) -> Self {
        Self {
            task_service,
            model_config_service,
            external_mcp_config_service,
            skill_service,
            run_service,
            ask_user_prompt_service,
            mcp_catalog_service,
        }
    }
}
