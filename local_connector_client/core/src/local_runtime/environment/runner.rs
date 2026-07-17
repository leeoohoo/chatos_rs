// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use chatos_ai_runtime::{
    build_responses_text_input, run_compatible_prompt_with, AiRequestHandler, SimplePromptOptions,
};
use chatos_plugin_management_sdk::{required_agent_prompt_vendor, SystemAgentKey};

use crate::local_runtime::capabilities::resolver::LocalCapabilityResolver;
use crate::local_runtime::load_installed_agent_prompt;
use crate::local_runtime::model::build_local_model_config;
use crate::model_configs::resolve_local_model_runtime;
use crate::LocalRuntime;

use super::json_output::parse_model_json;
use super::prompt::{environment_analysis_prompt, normalize_analysis};
use super::scan::scan_local_project;
use super::LocalEnvironmentAnalysisResult;

pub(crate) async fn run_local_environment_analysis(
    runtime: LocalRuntime,
    owner_user_id: String,
    project_id: String,
    model_config_id: String,
    run_id: String,
) -> Result<(), String> {
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?;
    let project = database
        .get_project(project_id.as_str(), owner_user_id.as_str())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Local project was not found".to_string())?;
    let capability = LocalCapabilityResolver::new(database, owner_user_id.as_str())
        .resolve_agent(SystemAgentKey::ProjectManagementAgent)
        .await?;
    if !capability.agent_enabled {
        return Err("Project Environment Agent is disabled by Plugin Management".to_string());
    }
    capability
        .ensure_required_available()
        .map_err(|error| error.to_string())?;

    let (project_root, resolved_model, thinking_level) = {
        let state = runtime.state.read().await;
        let root = resolve_project_root(&state, &project)?;
        let model = resolve_local_model_runtime(&state, owner_user_id.as_str(), &model_config_id)
            .map_err(|error| error.to_string())?;
        let thinking = state
            .model_configs
            .settings
            .environment_initialization_thinking_level
            .clone();
        (root, model, thinking)
    };
    let evidence = scan_local_project(project_root.clone()).await?;
    let prompt_vendor = required_agent_prompt_vendor(
        resolved_model.prompt_vendor.as_deref(),
        resolved_model.provider.as_str(),
    )
    .map_err(|error| error.to_string())?;
    let installed_prompt = load_installed_agent_prompt(
        &runtime,
        SystemAgentKey::ProjectManagementAgent,
        prompt_vendor,
    )
    .await
    .map_err(|error| error.to_string())?;
    database
        .update_local_environment_progress(
            owner_user_id.as_str(),
            project_id.as_str(),
            Some(run_id.as_str()),
            "running_agent",
            "running",
            Some(55),
            "Running Project Environment Agent locally",
            None,
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    let capability_prompt =
        capability.compose_provider_skills_prompt(std::iter::empty::<&str>(), Some("zh-CN"));
    let prompt = environment_analysis_prompt(
        project.project_id.as_str(),
        project.project_name.as_str(),
        &evidence,
        capability_prompt.as_deref(),
    )?;
    let model = build_local_model_config(
        resolved_model,
        Some(installed_prompt.content),
        thinking_level,
        Some(0.1),
        true,
        Some(project_root.display().to_string()),
    );
    let response = run_compatible_prompt_with(
        &AiRequestHandler::new(),
        &model,
        prompt.as_str(),
        SimplePromptOptions {
            max_attempts: Some(2),
            max_output_tokens: Some(8_000),
            ..Default::default()
        },
        build_responses_text_input,
    )
    .await?;
    let analysis = normalize_analysis(parse_model_json::<LocalEnvironmentAnalysisResult>(
        response.content.as_str(),
    )?)?;
    database
        .update_local_environment_progress(
            owner_user_id.as_str(),
            project_id.as_str(),
            Some(run_id.as_str()),
            "saving_result",
            "running",
            Some(90),
            "Saving environment plan to local SQLite",
            None,
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    database
        .finish_local_environment_analysis(
            owner_user_id.as_str(),
            project_id.as_str(),
            run_id.as_str(),
            &analysis,
        )
        .await
        .map_err(|error| error.to_string())?;
    database
        .update_local_environment_progress(
            owner_user_id.as_str(),
            project_id.as_str(),
            Some(run_id.as_str()),
            "completed",
            "succeeded",
            Some(100),
            "Local environment analysis completed",
            None,
            true,
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn resolve_project_root(
    state: &crate::LocalState,
    project: &crate::local_runtime::storage::LocalProjectRecord,
) -> Result<PathBuf, String> {
    let workspace = state
        .workspace_by_id(project.workspace_id.as_str())
        .ok_or_else(|| "Local project workspace is not registered".to_string())?;
    let root = project
        .root_relative_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != ".")
        .map(|relative| workspace.absolute_root.join(relative))
        .unwrap_or_else(|| workspace.absolute_root.clone());
    let workspace_root = std::fs::canonicalize(workspace.absolute_root.as_path())
        .map_err(|error| format!("Resolve local workspace failed: {error}"))?;
    let project_root = std::fs::canonicalize(root.as_path())
        .map_err(|error| format!("Resolve local project root failed: {error}"))?;
    if !project_root.starts_with(workspace_root) {
        return Err("Local project root escapes the registered workspace".to_string());
    }
    Ok(project_root)
}
