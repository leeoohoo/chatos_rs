// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::project_management_contract::args::InitProjectArgs;
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{
    now_rfc3339, ProjectProfileRecord, UpdateProjectRequest, UpsertProjectProfileRequest,
};
use crate::services::dependency_graph;
use crate::state::AppState;

use super::{decode_value, ensure_project_writable, require_project_access, tool_text_result};

pub(super) async fn get_project_overview(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
) -> Result<Value, String> {
    let project = require_project_access(state, project_id, current_user).await?;
    let profile = state
        .store
        .get_project_profile(project_id)
        .await?
        .unwrap_or_else(|| {
            let now = now_rfc3339();
            ProjectProfileRecord {
                project_id: project_id.to_string(),
                creator_user_id: None,
                creator_username: None,
                creator_display_name: None,
                owner_user_id: None,
                owner_username: None,
                owner_display_name: None,
                background: None,
                introduction: None,
                created_at: now.clone(),
                updated_at: now,
            }
        });
    Ok(tool_text_result(
        json!({ "project": project, "profile": profile }),
    ))
}

pub(super) async fn initialize_project(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: InitProjectArgs = decode_value(arguments)?;
    let project = require_project_access(state, project_id, current_user).await?;
    ensure_project_writable(&project)?;
    let project = state
        .store
        .update_project(
            project_id,
            UpdateProjectRequest {
                name: args.name,
                root_path: args.root_path,
                git_url: args.git_url,
                description: args.description,
            },
        )
        .await?
        .ok_or_else(|| format!("项目不存在: {project_id}"))?;
    let existing_profile = state.store.get_project_profile(project_id).await?;
    let profile = state
        .store
        .upsert_project_profile(
            project_id,
            UpsertProjectProfileRequest {
                background: args.background.or_else(|| {
                    existing_profile
                        .as_ref()
                        .and_then(|profile| profile.background.clone())
                }),
                introduction: args.introduction.or_else(|| {
                    existing_profile
                        .as_ref()
                        .and_then(|profile| profile.introduction.clone())
                }),
            },
            current_user,
        )
        .await?;
    Ok(tool_text_result(
        json!({ "project": project, "profile": profile }),
    ))
}

pub(super) async fn get_project_dependency_graph(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
) -> Result<Value, String> {
    require_project_access(state, project_id, current_user).await?;
    let graph = dependency_graph::project_dependency_graph(&state.store, project_id, false).await?;
    Ok(tool_text_result(json!(graph)))
}
