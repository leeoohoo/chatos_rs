// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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

        let workspace_root = sandbox_workspace_root(effective_workspace_dir)?;
        let base_url = self.sandbox_manager_base_url_for_task(task).await?;
        let ttl_seconds = self.effective_sandbox_lease_ttl_seconds().await?;
        let client =
            SandboxManagerClient::new(base_url, SandboxManagerAuth::from_config(&self.config))?;

        self.append_sandbox_event(
            run,
            "sandbox_requested",
            "正在申请沙箱",
            Some(json!({
                "workspace_root": workspace_root.to_string_lossy(),
                "ttl_seconds": ttl_seconds,
            })),
        )
        .await;

        let response = match client
            .create_lease(task, run, workspace_root.as_path(), ttl_seconds)
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
