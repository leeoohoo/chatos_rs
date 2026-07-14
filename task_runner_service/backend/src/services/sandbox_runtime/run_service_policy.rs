// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use chatos_sandbox_contract::{EffectiveSandboxPolicy, SandboxLeasePolicyRequest};

use super::*;

impl RunService {
    pub(in crate::services) async fn effective_sandbox_policy_for_task(
        &self,
        task: &TaskRecord,
    ) -> Result<bool, String> {
        if !task.mcp_config.enabled {
            return Ok(false);
        }
        let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
        if project_id != crate::models::PUBLIC_PROJECT_ID
            && super::project_management_api_client::project_service_enabled(&self.config)
        {
            match super::project_management_api_client::sync_get_project(
                &self.config,
                project_id.as_str(),
            )
            .await
            {
                Ok(Some(project)) => {
                    let source_type = project
                        .source_type
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or("local");
                    if source_type.eq_ignore_ascii_case("cloud") {
                        return Ok(true);
                    }
                    if source_type.eq_ignore_ascii_case("local")
                        || source_type.eq_ignore_ascii_case("local_connector")
                    {
                        match super::project_management_api_client::get_project_sandbox_enabled(
                            &self.config,
                            project_id.as_str(),
                        )
                        .await
                        {
                            Ok(enabled) => return Ok(enabled),
                            Err(err) => warn!(
                                project_id = project_id.as_str(),
                                error = err.as_str(),
                                "failed to load project sandbox policy; falling back to task/runtime settings"
                            ),
                        }
                    }
                }
                Ok(None) => {}
                Err(err) => warn!(
                    project_id = project_id.as_str(),
                    error = err.as_str(),
                    "failed to load project source type for sandbox policy"
                ),
            }
        }
        if let Some(enabled) = task.mcp_config.sandbox_enabled {
            return Ok(enabled);
        }
        self.effective_sandbox_enabled().await
    }

    pub(in crate::services) async fn should_route_task_to_sandbox(
        &self,
        task: &TaskRecord,
    ) -> Result<bool, String> {
        if !task.mcp_config.enabled {
            return Ok(false);
        }
        let sandbox_enabled = self.effective_sandbox_policy_for_task(task).await?;
        Ok(sandbox_enabled && task_requires_sandbox(task))
    }

    pub(in crate::services) async fn prepare_sandbox_if_needed(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        effective_workspace_dir: &str,
    ) -> Result<Option<SandboxRuntimeContext>, String> {
        if !self.should_route_task_to_sandbox(task).await? {
            return Ok(None);
        }

        let route = match self.sandbox_route_for_task(task).await {
            Ok(route) => route,
            Err(err) => {
                self.append_sandbox_event(
                    run,
                    "sandbox_failed",
                    format!("解析沙箱路由或镜像失败: {err}"),
                    Some(json!({
                        "requires_execution": task.mcp_config.requires_execution,
                        "project_id": task.project_id.as_str(),
                    })),
                )
                .await;
                return Err(err);
            }
        };
        let workspace_root = if is_local_connector_sandbox_manager(route.base_url.as_str()) {
            sandbox_workspace_root(self.config.default_workspace_dir.as_str())?
        } else {
            sandbox_workspace_root(effective_workspace_dir)?
        };
        let base_url = route.base_url.clone();
        let ttl_seconds = self.effective_sandbox_lease_ttl_seconds().await?;
        let client = SandboxManagerClient::new(base_url, route.auth.clone())?;

        self.append_sandbox_event(
            run,
            "sandbox_requested",
            "正在申请沙箱",
            Some(json!({
                "workspace_root": workspace_root.to_string_lossy(),
                "ttl_seconds": ttl_seconds,
                "provider": route.provider.as_str(),
                "image_id": route.image_id.as_deref(),
                "requires_execution": task.mcp_config.requires_execution,
                "requested_policy": route.policy,
            })),
        )
        .await;

        let response = match client
            .create_lease(
                task,
                run,
                workspace_root.as_path(),
                ttl_seconds,
                route.image_id.as_deref(),
                route.policy.clone(),
            )
            .await
        {
            Ok(response) => response,
            Err(err) => {
                self.append_sandbox_event(
                    run,
                    "sandbox_failed",
                    format!("申请沙箱失败: {err}"),
                    None,
                )
                .await;
                return Err(err);
            }
        };
        if response.is_waiting() {
            self.append_sandbox_event(
                run,
                "sandbox_queued",
                format!("沙箱正在排队或启动中: {}", response.status_label()),
                Some(json!({
                    "sandbox_id": response.sandbox_id.as_str(),
                    "lease_id": response.lease_id.as_str(),
                    "status": response.status_label(),
                })),
            )
            .await;
        }

        let response = match client.wait_until_ready(response).await {
            Ok(response) => response,
            Err(err) => {
                self.append_sandbox_event(
                    run,
                    "sandbox_failed",
                    format!("等待沙箱就绪失败: {err}"),
                    None,
                )
                .await;
                return Err(err);
            }
        };

        if let Err(err) =
            validate_effective_policy_is_not_broader(&route.policy, &response.effective_policy)
        {
            let _ = client.release_response(&response, false, true).await;
            self.append_sandbox_event(
                run,
                "sandbox_failed",
                err.clone(),
                Some(json!({
                    "requested_policy": route.policy,
                    "effective_policy": response.effective_policy,
                })),
            )
            .await;
            return Err(err);
        }

        self.append_sandbox_event(
            run,
            "sandbox_policy_resolved",
            "sandbox policy resolved",
            Some(json!({
                "requested_policy": route.policy,
                "effective_policy": response.effective_policy,
            })),
        )
        .await;

        let context = match SandboxRuntimeContext::from_response(
            response,
            workspace_root.as_path(),
            client.base_url.as_str(),
            client.auth.clone(),
        ) {
            Ok(context) => context,
            Err(err) => {
                self.append_sandbox_event(
                    run,
                    "sandbox_failed",
                    format!("沙箱响应无效: {err}"),
                    None,
                )
                .await;
                return Err(err);
            }
        };

        let should_copy_workspace =
            !is_local_connector_sandbox_manager(context.manager_base_url.as_str());
        let baseline_workspace = match sandbox_baseline_workspace(&context.run_workspace) {
            Ok(path) => path,
            Err(err) => {
                let _ = client.release(&context, true, true).await;
                self.append_sandbox_event(
                    run,
                    "sandbox_failed",
                    format!("准备沙箱 baseline 路径失败: {err}"),
                    Some(context.to_metadata()),
                )
                .await;
                return Err(err);
            }
        };
        if should_copy_workspace {
            if let Err(err) =
                copy_workspace_to_sandbox(effective_workspace_dir, baseline_workspace.as_str())
            {
                let _ = client.release(&context, true, true).await;
                self.append_sandbox_event(
                    run,
                    "sandbox_failed",
                    format!("复制工作区 baseline 失败: {err}"),
                    Some(context.to_metadata()),
                )
                .await;
                return Err(err);
            }
            if let Err(err) =
                copy_workspace_to_sandbox(effective_workspace_dir, &context.run_workspace)
            {
                let _ = client.release(&context, true, true).await;
                self.append_sandbox_event(
                    run,
                    "sandbox_failed",
                    format!("复制工作区到沙箱失败: {err}"),
                    Some(context.to_metadata()),
                )
                .await;
                return Err(err);
            }
        }

        match client.health(&context).await {
            Ok(health) if health.ok => {}
            Ok(health) => {
                let _ = client.release(&context, true, true).await;
                let message = format!("沙箱健康检查失败: {}", health.message);
                self.append_sandbox_event(
                    run,
                    "sandbox_failed",
                    message.clone(),
                    Some(json!({
                        "sandbox": context.to_metadata(),
                        "health": health.raw,
                    })),
                )
                .await;
                return Err(message);
            }
            Err(err) => {
                let _ = client.release(&context, true, true).await;
                self.append_sandbox_event(
                    run,
                    "sandbox_failed",
                    format!("沙箱健康检查失败: {err}"),
                    Some(context.to_metadata()),
                )
                .await;
                return Err(err);
            }
        }

        attach_sandbox_context_to_run(run, &context);
        run.updated_at = now_rfc3339();
        self.store.save_run(run.clone()).await?;
        self.append_sandbox_event(
            run,
            "sandbox_ready",
            "沙箱已就绪，文件和终端 MCP 将使用沙箱服务",
            Some(json!({
                "sandbox": context.to_metadata(),
                "baseline_workspace": baseline_workspace,
            })),
        )
        .await;
        info!(
            task_id = task.id.as_str(),
            run_id = run.id.as_str(),
            sandbox_id = context.sandbox_id.as_str(),
            lease_id = context.lease_id.as_str(),
            run_workspace = context.run_workspace.as_str(),
            "task runner prepared sandbox"
        );

        Ok(Some(context))
    }
}

fn validate_effective_policy_is_not_broader(
    requested: &SandboxLeasePolicyRequest,
    effective: &EffectiveSandboxPolicy,
) -> Result<(), String> {
    if let Some(requested_backend) = requested.sandbox_mode {
        if effective.sandbox_mode != requested_backend {
            return Err(format!(
                "sandbox effective backend {} does not match requested {}",
                effective.sandbox_mode.as_str(),
                requested_backend.as_str()
            ));
        }
    }

    if let Some(requested_profile) = requested.permission_profile_id {
        if !effective
            .permission_profile_id
            .is_no_broader_than(requested_profile)
        {
            return Err(format!(
                "sandbox effective permission profile {} is broader than requested {}",
                effective.permission_profile_id.as_str(),
                requested_profile.as_str()
            ));
        }
    }

    if let Some(requested_policy) = requested.approval_policy {
        if !effective
            .approval_policy
            .is_no_broader_than(requested_policy)
        {
            return Err(format!(
                "sandbox effective approval policy {} is broader than requested {}",
                effective.approval_policy.as_str(),
                requested_policy.as_str()
            ));
        }
    }

    if let Some(requested_reviewer) = requested.approval_reviewer {
        if !effective
            .approval_reviewer
            .is_no_broader_than(requested_reviewer)
        {
            return Err(format!(
                "sandbox effective approval reviewer {} is broader than requested {}",
                effective.approval_reviewer.as_str(),
                requested_reviewer.as_str()
            ));
        }
    }

    let requested_roots = normalized_root_set(&requested.additional_writable_roots);
    let effective_roots = normalized_root_set(&effective.additional_writable_roots);
    if !effective_roots.is_subset(&requested_roots) {
        let extra = effective_roots
            .difference(&requested_roots)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "sandbox effective additional writable roots exceed requested roots: {extra}"
        ));
    }

    Ok(())
}

fn normalized_root_set(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod policy_validation_tests {
    use super::*;
    use chatos_sandbox_contract::{
        ApprovalPolicy, ApprovalReviewer, PermissionProfileId, SandboxBackendKind,
    };

    fn request() -> SandboxLeasePolicyRequest {
        SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::Docker),
            permission_profile_id: Some(PermissionProfileId::WorkspaceWrite),
            approval_policy: Some(ApprovalPolicy::OnRequest),
            approval_reviewer: Some(ApprovalReviewer::User),
            policy_revision: None,
            additional_writable_roots: vec!["C:/project/cache".to_string()],
        }
    }

    fn effective() -> EffectiveSandboxPolicy {
        EffectiveSandboxPolicy {
            sandbox_mode: SandboxBackendKind::Docker,
            permission_profile_id: PermissionProfileId::WorkspaceWrite,
            approval_policy: ApprovalPolicy::OnRequest,
            approval_reviewer: ApprovalReviewer::User,
            policy_revision: None,
            additional_writable_roots: vec!["C:/project/cache".to_string()],
        }
    }

    #[test]
    fn effective_policy_matching_or_stricter_than_request_is_allowed() {
        let mut effective = effective();
        effective.permission_profile_id = PermissionProfileId::ReadOnly;

        validate_effective_policy_is_not_broader(&request(), &effective).expect("valid policy");
    }

    #[test]
    fn effective_policy_rejects_broader_permission_profile() {
        let mut requested = request();
        requested.permission_profile_id = Some(PermissionProfileId::ReadOnly);
        let mut effective = effective();
        effective.permission_profile_id = PermissionProfileId::WorkspaceWrite;

        let err = validate_effective_policy_is_not_broader(&requested, &effective)
            .expect_err("workspace write is broader than read only");

        assert!(err.contains("permission profile"));
    }

    #[test]
    fn effective_policy_rejects_backend_mismatch() {
        let mut effective = effective();
        effective.sandbox_mode = SandboxBackendKind::LocalProcess;

        let err = validate_effective_policy_is_not_broader(&request(), &effective)
            .expect_err("backend mismatch should fail");

        assert!(err.contains("backend"));
    }

    #[test]
    fn effective_policy_rejects_broader_approval_policy_or_reviewer() {
        let mut effective_policy = effective();
        effective_policy.approval_policy = ApprovalPolicy::Never;
        assert!(
            validate_effective_policy_is_not_broader(&request(), &effective_policy)
                .expect_err("never is broader")
                .contains("approval policy")
        );

        let mut effective_reviewer = effective();
        effective_reviewer.approval_reviewer = ApprovalReviewer::AutoReview;
        assert!(
            validate_effective_policy_is_not_broader(&request(), &effective_reviewer)
                .expect_err("auto review is broader")
                .contains("approval reviewer")
        );
    }

    #[test]
    fn effective_policy_rejects_unrequested_extra_writable_roots() {
        let mut effective = effective();
        effective
            .additional_writable_roots
            .push("C:/outside".to_string());

        let err = validate_effective_policy_is_not_broader(&request(), &effective)
            .expect_err("extra roots should fail");

        assert!(err.contains("additional writable roots"));
        assert!(err.contains("C:/outside"));
    }
}
