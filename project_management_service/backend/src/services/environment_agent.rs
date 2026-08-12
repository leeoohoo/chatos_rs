// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::*;
use crate::state::AppState;
use crate::user_model_runtime_client::resolve_default_environment_initialization_model_runtime;
use serde_json::{json, Value};

use super::runtime_environment::{
    enforce_project_runtime_boundary, ensure_runtime_environment_for_project,
};

mod agent_prompt;
mod mcp_management_gateway;
mod mcp_servers;
mod memory;
mod progress;
mod routing;
mod source_snapshot;
mod tool_provider;

pub use self::progress::get_project_runtime_environment_progress;

use self::agent_prompt::resolve_project_environment_agent_prompt;
use self::mcp_management_gateway::{
    resolve_existing_project_environment_mcp, resolve_project_environment_mcp,
};
use self::mcp_servers::{
    create_sandbox_image_from_plan, ensure_agent_required_tools_available,
    get_local_project_compose_environment_status, get_sandbox_image_catalog,
    prepare_sandbox_dependency_images, restart_local_project_compose_environment,
    start_local_project_compose_environment, stop_local_project_compose_environment,
};
use self::memory::build_project_agent_memory;
use self::routing::{
    resolve_runtime_environment_plan, RuntimeEnvironmentDecision, RuntimeEnvironmentPlan,
    StopDecision,
};
pub(crate) use self::tool_provider::{
    ensure_project_environment_agent_run, ProjectEnvironmentToolProvider,
};
const LOCAL_SANDBOX_IMAGE_MCP_PATH: &str = "/api/local/sandbox/images/mcp";
const PROJECT_COMPOSE_FILE_PATH: &str = ".chatos/runtime-environment/docker-compose.chatos.yml";

pub(crate) fn refresh_project_runtime_compose_config(
    project_id: &str,
    environment: &mut ProjectRuntimeEnvironmentRecord,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> Result<bool, String> {
    let previous = environment
        .generated_config_files
        .iter()
        .find(|file| file.path == PROJECT_COMPOSE_FILE_PATH)
        .map(|file| {
            (
                file.format.clone(),
                file.content.clone(),
                file.description.clone(),
                file.source_files.clone(),
            )
        });
    if images
        .iter()
        .any(|image| image.service_role == RuntimeServiceRole::Application)
    {
        tool_provider::compose::upsert_project_compose_config_file(
            project_id,
            &mut environment.generated_config_files,
            &environment.environment_variables,
            &environment.required_services,
            images,
        )?;
    } else {
        environment
            .generated_config_files
            .retain(|file| file.path != PROJECT_COMPOSE_FILE_PATH);
    }
    let current = environment
        .generated_config_files
        .iter()
        .find(|file| file.path == PROJECT_COMPOSE_FILE_PATH)
        .map(|file| {
            (
                file.format.clone(),
                file.content.clone(),
                file.description.clone(),
                file.source_files.clone(),
            )
        });
    Ok(previous != current)
}

#[cfg(test)]
mod compose_refresh_tests {
    use super::*;

    fn dependency_image(environment_key: &str) -> ProjectRuntimeEnvironmentImageRecord {
        let now = "2026-08-02T00:00:00Z".to_string();
        let mut image = ProjectRuntimeEnvironmentImageRecord {
            id: format!("image-{environment_key}"),
            project_id: "project-1".to_string(),
            environment_key: environment_key.to_string(),
            environment_type: "service".to_string(),
            display_name: environment_key.to_string(),
            service_id: String::new(),
            service_role: RuntimeServiceRole::Unknown,
            source_root: ".".to_string(),
            component_kind: "service".to_string(),
            startup_command: None,
            test_command: None,
            depends_on: Vec::new(),
            auto_start: true,
            mcp_policy: ProgramManagedMcpPolicy::default(),
            image_id: None,
            image_ref: None,
            image_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            features: empty_array(),
            ports: empty_array(),
            env_vars: empty_object(),
            dockerfile: None,
            custom_build_script: None,
            status: "ready".to_string(),
            error: None,
            created_at: now.clone(),
            updated_at: now,
        };
        crate::services::runtime_environment::apply_program_managed_image_policy(&mut image);
        image
    }

    #[test]
    fn dependency_only_bootstrap_does_not_require_an_application_compose_plan() {
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::PendingImageBuild,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::Harness,
            analysis_summary: None,
            not_runnable_reason: None,
            execution_service_id: Some("workspace".to_string()),
            detected_stack: empty_object(),
            required_services: serde_json::json!([
                {"type": "postgres"},
                {"type": "redis"}
            ]),
            env_vars: empty_object(),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "2026-08-02T00:00:00Z".to_string(),
            updated_at: "2026-08-02T00:00:00Z".to_string(),
        };
        let images = vec![dependency_image("postgres"), dependency_image("redis")];

        assert!(!refresh_project_runtime_compose_config(
            "project-1",
            &mut environment,
            images.as_slice(),
        )
        .expect("dependency-only bootstrap must not require a phantom application"));
        assert!(environment.generated_config_files.is_empty());
    }
}

mod runtime;
pub(crate) use runtime::analysis::cloud_agent_profile;

pub async fn start_project_runtime_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    runtime::lifecycle::start_project_runtime_environment_impl(state, project, user_access_token)
        .await
}

pub async fn get_project_runtime_environment_deployment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<Value, String> {
    runtime::lifecycle::get_project_runtime_environment_deployment_impl(
        state,
        project,
        user_access_token,
    )
    .await
}

pub async fn stop_project_runtime_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    runtime::lifecycle::stop_project_runtime_environment_impl(state, project, user_access_token)
        .await
}

pub async fn restart_project_runtime_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    runtime::lifecycle::restart_project_runtime_environment_impl(state, project, user_access_token)
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
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
    prefer_china_mirrors: bool,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    runtime::analysis::analyze_project_runtime_environment_impl(
        state,
        project,
        user_access_token,
        run_id,
        analysis_requirement,
        selected_dependencies,
        prefer_china_mirrors,
    )
    .await
}

pub(super) fn compose_dependency_image_ref(
    image: &ProjectRuntimeEnvironmentImageRecord,
) -> Option<String> {
    runtime::lifecycle::compose_dependency_image_ref_impl(image)
}

pub(super) fn runtime_application_service_id(
    image: &ProjectRuntimeEnvironmentImageRecord,
    _index: usize,
) -> String {
    crate::services::runtime_environment::program_managed_service_id(image)
}
