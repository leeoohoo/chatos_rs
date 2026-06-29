use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{McpServerInfo, McpServerToolProfileInfo};

use super::chatos_async_planner::enrich_tool_schemas_for_async_planner;
use super::support::{
    agent_tool_allowed_for_profile, create_model_config_schema, create_task_schema,
    create_tasks_with_prerequisites_schema, empty_object_schema,
    enrich_tool_schemas_with_model_configs, filter_model_configs_for_user, get_skill_detail_schema,
    prerequisite_task_ids_schema, prompt_status_values, required_object_schema, run_status_values,
    search_installed_skills_schema, task_status_values, tool_definition,
    update_model_config_schema, update_task_schema,
};
use super::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpRequestContext, McpToolProfile,
    TaskRunnerMcpService, CHATOS_ASYNC_PLANNER_TOOL_PROFILE, TASK_RUNNER_MCP_ENDPOINT_PATH,
    TASK_RUNNER_MCP_SERVER_NAME, TASK_RUNNER_MCP_STDIO_ARGS, TASK_RUNNER_MCP_STDIO_COMMAND,
};

mod dispatch;
mod tool_definitions;
