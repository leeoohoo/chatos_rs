// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::*;

use super::{apply_program_managed_image_policy, ensure_workspace_execution_record};

pub fn enforce_project_runtime_boundary(
    project: &ProjectRecord,
    environment: &mut ProjectRuntimeEnvironmentRecord,
    images: &mut Vec<ProjectRuntimeEnvironmentImageRecord>,
) -> bool {
    let mut changed = false;
    if environment
        .not_runnable_reason
        .as_deref()
        .map(str::trim)
        .is_some_and(|reason| !reason.is_empty())
        && !matches!(
            environment.status,
            ProjectRuntimeEnvironmentStatus::Disabled
                | ProjectRuntimeEnvironmentStatus::Analyzing
                | ProjectRuntimeEnvironmentStatus::Failed
                | ProjectRuntimeEnvironmentStatus::NotRunnable
        )
    {
        environment.status = ProjectRuntimeEnvironmentStatus::NotRunnable;
        environment.analysis_summary = environment.not_runnable_reason.clone();
        changed = true;
    }
    if !environment.sandbox_enabled {
        if environment.sandbox_provider != RuntimeEnvironmentProvider::None {
            environment.sandbox_provider = RuntimeEnvironmentProvider::None;
            changed = true;
        }
        if environment.file_provider != RuntimeEnvironmentProvider::None {
            environment.file_provider = RuntimeEnvironmentProvider::None;
            changed = true;
        }
        if !images.is_empty() {
            images.clear();
            changed = true;
        }
        if environment.execution_service_id.take().is_some() {
            changed = true;
        }
        if changed {
            environment.updated_at = now_rfc3339();
        }
        return changed;
    }

    let desired_sandbox_provider = match project.source_type {
        ProjectSourceType::Cloud => RuntimeEnvironmentProvider::CloudSandboxManager,
        ProjectSourceType::Local | ProjectSourceType::LocalConnector => {
            if chatos_project_execution::parse_local_connector_workspace_root(
                project.root_path.as_deref().unwrap_or_default(),
            )
            .is_none()
            {
                environment.status = ProjectRuntimeEnvironmentStatus::NotRunnable;
                environment.not_runnable_reason =
                    Some("本地项目缺少有效的 Local Connector 逻辑 Workspace".to_string());
                environment.analysis_summary = environment.not_runnable_reason.clone();
                RuntimeEnvironmentProvider::None
            } else if environment.sandbox_provider == RuntimeEnvironmentProvider::LocalConnector {
                RuntimeEnvironmentProvider::LocalConnector
            } else {
                // A local project may only enter a local sandbox after an online pairing was
                // resolved. Stale cloud state is discarded instead of becoming a fallback.
                RuntimeEnvironmentProvider::None
            }
        }
    };
    let desired_file_provider = match project.source_type {
        ProjectSourceType::Cloud => RuntimeEnvironmentProvider::Harness,
        ProjectSourceType::Local | ProjectSourceType::LocalConnector
            if desired_sandbox_provider == RuntimeEnvironmentProvider::LocalConnector =>
        {
            RuntimeEnvironmentProvider::LocalConnector
        }
        ProjectSourceType::Local | ProjectSourceType::LocalConnector => {
            RuntimeEnvironmentProvider::None
        }
    };
    if environment.sandbox_provider != desired_sandbox_provider {
        environment.sandbox_provider = desired_sandbox_provider;
        changed = true;
    }
    if environment.file_provider != desired_file_provider {
        environment.file_provider = desired_file_provider;
        changed = true;
    }
    if environment.status == ProjectRuntimeEnvironmentStatus::NotRunnable {
        if !images.is_empty() {
            images.clear();
            changed = true;
        }
        if environment.execution_service_id.take().is_some() {
            changed = true;
        }
        if environment.analysis_summary != environment.not_runnable_reason {
            environment.analysis_summary = environment.not_runnable_reason.clone();
            changed = true;
        }
        if changed {
            environment.updated_at = now_rfc3339();
        }
        return changed;
    }
    if desired_sandbox_provider == RuntimeEnvironmentProvider::None {
        if !images.is_empty() {
            images.clear();
            changed = true;
        }
        if environment.execution_service_id.take().is_some() {
            changed = true;
        }
        if changed {
            environment.updated_at = now_rfc3339();
        }
        return changed;
    }

    let legacy_target = images
        .iter()
        .find(|image| {
            image.service_role == RuntimeServiceRole::Application
                && image.mcp_policy.attachment == RuntimeMcpAttachment::WorkspaceGatewayTarget
        })
        .cloned();
    if ensure_workspace_execution_record(environment, images, legacy_target.as_ref()) {
        changed = true;
    }

    let mut workspace_image_reset = false;
    for image in images.iter_mut() {
        let mut image_changed = apply_program_managed_image_policy(image);
        let wrong_provider = image.image_provider != desired_sandbox_provider;
        if wrong_provider {
            image.image_provider = desired_sandbox_provider;
            changed = true;
            image_changed = true;
        }
        if wrong_provider && image.service_role == RuntimeServiceRole::Workspace {
            image.image_id = None;
            image.image_ref = None;
            image.status = "planned".to_string();
            image.error = None;
            workspace_image_reset = true;
            changed = true;
            image_changed = true;
        }
        if image_changed {
            changed = true;
            image.updated_at = now_rfc3339();
        }
    }

    let workspace_requires_build = images.iter().any(|image| {
        image.service_role == RuntimeServiceRole::Workspace
            && (image
                .image_id
                .as_deref()
                .or(image.image_ref.as_deref())
                .is_none_or(|value| value.trim().is_empty())
                || !matches!(
                    image.status.trim().to_ascii_lowercase().as_str(),
                    "ready" | "available" | "local" | "succeeded" | "completed" | "running"
                ))
    });
    if workspace_requires_build
        && !matches!(
            environment.status,
            ProjectRuntimeEnvironmentStatus::Disabled
                | ProjectRuntimeEnvironmentStatus::Analyzing
                | ProjectRuntimeEnvironmentStatus::NotRunnable
                | ProjectRuntimeEnvironmentStatus::Failed
                | ProjectRuntimeEnvironmentStatus::PendingImageBuild
        )
    {
        environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
        if environment.analysis_summary.is_none() {
            environment.analysis_summary =
                Some("项目组件和依赖拓扑已保留，等待生成唯一工作区执行镜像。".to_string());
        }
        changed = true;
    }

    let execution_service_id = images
        .iter()
        .find(|image| image.service_role == RuntimeServiceRole::Workspace)
        .map(|image| image.service_id.clone());
    if environment.execution_service_id != execution_service_id {
        environment.execution_service_id = execution_service_id;
        changed = true;
    }
    for image in images.iter_mut() {
        let desired_policy = if image.service_role == RuntimeServiceRole::Workspace {
            ProgramManagedMcpPolicy::workspace_target()
        } else {
            ProgramManagedMcpPolicy::default()
        };
        if image.mcp_policy != desired_policy {
            image.mcp_policy = desired_policy;
            image.updated_at = now_rfc3339();
            changed = true;
        }
    }

    if workspace_image_reset
        && !matches!(
            environment.status,
            ProjectRuntimeEnvironmentStatus::Disabled
                | ProjectRuntimeEnvironmentStatus::Analyzing
                | ProjectRuntimeEnvironmentStatus::NotRunnable
                | ProjectRuntimeEnvironmentStatus::Failed
        )
    {
        environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
        let missing_variables = environment
            .environment_variables
            .iter()
            .filter(|record| {
                record.required
                    && record
                        .effective_value
                        .as_deref()
                        .is_none_or(|value| value.trim().is_empty())
            })
            .map(|record| record.name.as_str())
            .collect::<Vec<_>>();
        let provider_label = match desired_sandbox_provider {
            RuntimeEnvironmentProvider::LocalConnector => "本地",
            RuntimeEnvironmentProvider::CloudSandboxManager => "云端",
            RuntimeEnvironmentProvider::None | RuntimeEnvironmentProvider::Harness => "目标",
        };
        environment.analysis_summary = Some(if missing_variables.is_empty() {
            format!(
                "运行环境分析和服务计划已保留；原有工作区镜像记录已作废，请生成{provider_label}工作区执行镜像。"
            )
        } else {
            format!(
                "运行环境分析和服务计划已保留；原有工作区镜像记录已作废，请先生成{provider_label}工作区执行镜像。镜像生成后仍需补充运行参数：{}。",
                missing_variables.join(", ")
            )
        });
    }
    if changed {
        environment.updated_at = now_rfc3339();
    }
    changed
}
