// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{extract::Path, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::models::project::Project;
use crate::services::{access_token_scope, project_management_api_client};

#[derive(Debug, Default, Deserialize)]
pub(super) struct ProjectRuntimeEnvironmentSettingsRequest {
    sandbox_enabled: Option<bool>,
}

fn is_cloud_project(project: &Project) -> bool {
    project
        .source_type
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("cloud"))
}

fn project_service_context() -> Result<(&'static Config, String), (StatusCode, Json<Value>)> {
    let cfg = Config::try_get().map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        )
    })?;
    let access_token = access_token_scope::get_current_access_token().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "current user access token is required" })),
        )
    })?;
    Ok((cfg, access_token))
}

pub(super) async fn get_project_runtime_environment(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let (cfg, access_token) = match project_service_context() {
        Ok(context) => context,
        Err(err) => return err,
    };
    let response = match project_management_api_client::get_project_service_runtime_environment(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
    )
    .await
    {
        Ok(response) => response,
        Err(err) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))),
    };
    (StatusCode::OK, Json(response))
}

pub(super) async fn update_project_runtime_environment_settings(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<ProjectRuntimeEnvironmentSettingsRequest>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    if is_cloud_project(&project) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "云端项目固定使用沙箱，不支持在本地项目设置中切换",
            })),
        );
    }

    let (cfg, access_token) = match project_service_context() {
        Ok(context) => context,
        Err(err) => return err,
    };
    let response =
        match project_management_api_client::update_project_service_runtime_environment_settings(
            cfg.project_service_base_url.as_str(),
            access_token.as_str(),
            project.id.as_str(),
            &project_management_api_client::UpdateProjectRuntimeEnvironmentSettingsRequest {
                sandbox_enabled: req.sandbox_enabled,
            },
        )
        .await
        {
            Ok(response) => response,
            Err(err) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))),
        };
    (StatusCode::OK, Json(response))
}

pub(super) async fn analyze_project_runtime_environment(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let (cfg, access_token) = match project_service_context() {
        Ok(context) => context,
        Err(err) => return err,
    };
    let response = match project_management_api_client::analyze_project_service_runtime_environment(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
    )
    .await
    {
        Ok(response) => response,
        Err(err) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))),
    };
    (StatusCode::OK, Json(response))
}

pub(super) async fn generate_project_runtime_environment_image(
    auth: AuthUser,
    Path((id, image_record_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    if !is_cloud_project(&project) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "本地项目镜像必须由本地客户端生成" })),
        );
    }
    let (cfg, access_token) = match project_service_context() {
        Ok(context) => context,
        Err(err) => return err,
    };
    let response =
        match project_management_api_client::generate_project_service_runtime_environment_image(
            cfg.project_service_base_url.as_str(),
            access_token.as_str(),
            project.id.as_str(),
            image_record_id.as_str(),
        )
        .await
        {
            Ok(response) => response,
            Err(err) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))),
        };
    (StatusCode::OK, Json(response))
}

pub(super) async fn get_project_runtime_environment_progress(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let (cfg, access_token) = match project_service_context() {
        Ok(context) => context,
        Err(err) => return err,
    };
    let response =
        match project_management_api_client::get_project_service_runtime_environment_progress(
            cfg.project_service_base_url.as_str(),
            access_token.as_str(),
            project.id.as_str(),
        )
        .await
        {
            Ok(response) => response,
            Err(err) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))),
        };
    (StatusCode::OK, Json(response))
}
