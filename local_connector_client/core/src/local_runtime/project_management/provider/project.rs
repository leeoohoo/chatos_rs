// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use chatos_project_mcp_contract::args::InitProjectArgs;
use serde_json::{json, Value};

use crate::local_runtime::project_management::UpsertLocalProjectProfileInput;

use super::{decode, normalized, LocalProjectManagementProvider};

pub(super) async fn get_overview(
    provider: &LocalProjectManagementProvider,
) -> Result<Value, String> {
    let project = provider
        .database
        .get_project(
            provider.project_id.as_str(),
            provider.owner_user_id.as_str(),
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "local project was not found".to_string())?;
    let profile = provider
        .database
        .get_local_project_profile(
            provider.owner_user_id.as_str(),
            provider.project_id.as_str(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(json!({ "project": project, "profile": profile }))
}

pub(super) async fn initialize(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: InitProjectArgs = decode(arguments)?;
    let root = normalized(args.root_path)
        .map(|value| safe_relative_root(value.as_str()))
        .transpose()?;
    provider
        .database
        .update_local_project_identity(
            provider.owner_user_id.as_str(),
            provider.project_id.as_str(),
            normalized(args.name).as_deref(),
            root.as_deref(),
        )
        .await
        .map_err(|error| error.to_string())?;
    provider
        .database
        .upsert_local_project_profile(
            provider.owner_user_id.as_str(),
            provider.project_id.as_str(),
            UpsertLocalProjectProfileInput {
                description: normalized(args.description),
                git_url: normalized(args.git_url),
                background: normalized(args.background),
                introduction: normalized(args.introduction),
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    get_overview(provider).await
}

pub(super) async fn get_dependency_graph(
    provider: &LocalProjectManagementProvider,
) -> Result<Value, String> {
    let plan = provider
        .database
        .local_project_plan(
            provider.owner_user_id.as_str(),
            provider.project_id.as_str(),
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    serde_json::to_value(plan.dependency_graph).map_err(|error| error.to_string())
}

fn safe_relative_root(value: &str) -> Result<String, String> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("local project root must stay inside the registered workspace".to_string());
    }
    Ok(value.trim_matches('/').to_string())
}
