// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::*;
use crate::state::AppState;
use crate::user_model_runtime_client::resolve_default_environment_initialization_model_runtime;
use chatos_agent::{AgentExecutor, AgentTurnMemory, AgentTurnRequest, PROJECT_ENVIRONMENT_AGENT};
use chatos_ai_runtime::ModelRuntimeConfig;
use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::{
    ResolveAgentCapabilitiesRequest, ResolvedAgentCapabilities, SystemAgentKey,
    PROJECT_ENVIRONMENT_MCP_RESOURCE_ID,
};
use serde_json::{json, Value};

use super::runtime_environment::{
    enforce_project_runtime_boundary, ensure_runtime_environment_for_project,
};

mod agent_prompt;
mod inspection;
mod mcp_servers;
mod memory;
mod progress;
mod routing;
mod tool_provider;

pub use self::progress::get_project_runtime_environment_progress;

use self::agent_prompt::resolve_project_environment_agent_prompt;
use self::inspection::{inspect_local_project, LocalProjectInspection};
use self::mcp_servers::{
    build_project_environment_mcp_executor, create_sandbox_image_from_plan,
    ensure_agent_required_tools_available, start_local_project_compose_environment,
};
use self::memory::{build_project_agent_memory, ProjectAgentMemory};
use self::routing::{
    provider_label, resolve_runtime_environment_routing, RoutingDecision, RoutingPlan, StopDecision,
};

const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";
const PROJECT_ENVIRONMENT_MCP_SERVER_NAME: &str = "project_environment";
const SANDBOX_IMAGE_MCP_SERVER_NAME: &str = "sandbox_images";
const CLOUD_SANDBOX_IMAGE_MCP_PATH: &str = "/api/sandbox-images/mcp";
const LOCAL_SANDBOX_IMAGE_MCP_PATH: &str = "/api/local/sandbox/images/mcp";
const PROJECT_COMPOSE_FILE_PATH: &str = ".chatos/runtime-environment/docker-compose.chatos.yml";

mod runtime;

pub async fn start_project_runtime_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    runtime::lifecycle::start_project_runtime_environment_impl(state, project, user_access_token)
        .await
}

pub async fn generate_project_runtime_environment_image(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    image_record_id: &str,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    runtime::image_generation::generate_project_runtime_environment_image_impl(
        state,
        project,
        user_access_token,
        image_record_id,
    )
    .await
}

pub async fn analyze_project_runtime_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    run_id: &str,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    runtime::analysis::analyze_project_runtime_environment_impl(
        state,
        project,
        user_access_token,
        run_id,
    )
    .await
}

pub(super) fn compose_dependency_image_ref(
    image: &ProjectRuntimeEnvironmentImageRecord,
) -> Option<String> {
    runtime::lifecycle::compose_dependency_image_ref_impl(image)
}
