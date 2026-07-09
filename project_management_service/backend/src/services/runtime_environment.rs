// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::*;
use crate::store::AppStore;

pub async fn ensure_runtime_environment_for_project(
    store: &AppStore,
    project: &ProjectRecord,
    sandbox_enabled: Option<bool>,
) -> Result<ProjectRuntimeEnvironmentRecord, String> {
    if let Some(mut existing) = store
        .get_project_runtime_environment(project.id.as_str())
        .await?
    {
        if let Some(sandbox_enabled) = sandbox_enabled {
            existing.sandbox_enabled = sandbox_enabled;
            existing.status = if sandbox_enabled {
                if existing.status == ProjectRuntimeEnvironmentStatus::Disabled {
                    ProjectRuntimeEnvironmentStatus::Pending
                } else {
                    existing.status
                }
            } else {
                ProjectRuntimeEnvironmentStatus::Disabled
            };
            if !sandbox_enabled {
                existing.sandbox_provider = RuntimeEnvironmentProvider::None;
                existing.file_provider = RuntimeEnvironmentProvider::None;
                existing.last_error = None;
            }
            existing.updated_at = now_rfc3339();
            let saved = store.upsert_project_runtime_environment(&existing).await?;
            if !sandbox_enabled {
                store
                    .replace_project_runtime_environment_images(project.id.as_str(), &[])
                    .await?;
            }
            return Ok(saved);
        }
        return Ok(existing);
    }
    let environment = default_runtime_environment_for_project(project, sandbox_enabled);
    store.upsert_project_runtime_environment(&environment).await
}

pub fn default_runtime_environment_for_project(
    project: &ProjectRecord,
    sandbox_enabled: Option<bool>,
) -> ProjectRuntimeEnvironmentRecord {
    let sandbox_enabled = sandbox_enabled.unwrap_or(true);
    let now = now_rfc3339();
    ProjectRuntimeEnvironmentRecord {
        project_id: project.id.clone(),
        status: if sandbox_enabled {
            ProjectRuntimeEnvironmentStatus::Pending
        } else {
            ProjectRuntimeEnvironmentStatus::Disabled
        },
        sandbox_enabled,
        sandbox_provider: RuntimeEnvironmentProvider::None,
        file_provider: RuntimeEnvironmentProvider::None,
        analysis_summary: None,
        not_runnable_reason: None,
        detected_stack: empty_object(),
        required_services: empty_array(),
        env_vars: empty_object(),
        last_agent_run_id: None,
        last_error: None,
        created_at: now.clone(),
        updated_at: now,
    }
}
