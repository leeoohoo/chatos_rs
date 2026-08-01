// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::models::{ModelConfigRecord, TaskRecord, PUBLIC_PROJECT_ID};
use crate::services::project_management_api_client::get_project_from_project_service;

pub(super) async fn resolve_model_runtime_for_task(
    config: &AppConfig,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
) -> Result<ModelConfigRecord, String> {
    ensure_cloud_task_project_execution(config, task).await?;
    let has_embedded_runtime =
        !model_config.api_key.trim().is_empty() && !model_config.base_url.trim().is_empty();
    if has_embedded_runtime {
        return Ok(model_config.clone());
    }
    Err(format!(
        "cloud_model_credentials_required: task runner model config {} must contain cloud-resident api_key and base_url; Local Connector credential lookup is disabled",
        model_config.id
    ))
}

pub(super) async fn ensure_cloud_task_project_execution(
    config: &AppConfig,
    task: &TaskRecord,
) -> Result<(), String> {
    let project_id = task.project_id.trim();
    if project_id.is_empty() || project_id == "0" || project_id == PUBLIC_PROJECT_ID {
        return Ok(());
    }
    get_project_from_project_service(config, project_id)
        .await?
        .ok_or_else(|| format!("task project not found: {project_id}"))?;
    Ok(())
}
