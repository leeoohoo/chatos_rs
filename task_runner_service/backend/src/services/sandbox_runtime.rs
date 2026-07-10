// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::Path;

use chatos_mcp_runtime::{BuiltinMcpKind, McpHttpServer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::models::{RunOutputChangesResponse, RunOutputDiffResponse, RunOutputFileChangeCounts};

use super::workspace_mcp::runtime_selected_builtin_kinds;
use super::*;

pub(super) const SANDBOX_MCP_SERVER_NAME: &str = "sandbox";
mod manager_client;
mod output;
mod workspace;

use manager_client::{CreateSandboxLeaseResponse, SandboxManagerAuth, SandboxManagerClient};
pub(super) use output::SandboxOutputReport;
use output::{
    normalize_output_relative_path, read_output_change_manifest_for_run, read_output_diff_file,
};
use workspace::{
    copy_workspace_to_sandbox, is_local_connector_sandbox_manager, sandbox_baseline_workspace,
    sandbox_workspace_root,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SandboxRuntimeContext {
    pub lease_id: String,
    pub sandbox_id: String,
    pub backend_id: Option<String>,
    pub agent_endpoint: String,
    pub agent_token: String,
    pub mcp_url: String,
    #[serde(default, skip_serializing)]
    pub manager_client_id: Option<String>,
    #[serde(default, skip_serializing)]
    pub manager_client_key: Option<String>,
    #[serde(default)]
    pub manager_base_url: String,
    pub run_workspace: String,
    pub workspace_root: String,
    pub expires_at: String,
}

impl SandboxRuntimeContext {
    pub(super) fn to_metadata(&self) -> Value {
        json!({
            "lease_id": self.lease_id,
            "sandbox_id": self.sandbox_id,
            "backend_id": self.backend_id,
            "agent_endpoint": self.agent_endpoint,
            "mcp_url": self.mcp_url,
            "manager_base_url": self.manager_base_url,
            "run_workspace": self.run_workspace,
            "workspace_root": self.workspace_root,
            "expires_at": self.expires_at,
        })
    }

    pub(super) fn to_mcp_server(&self, task: &TaskRecord, run: &TaskRunRecord) -> McpHttpServer {
        let mut headers = HashMap::new();
        headers.insert("X-Chatos-Sandbox-Id".to_string(), self.sandbox_id.clone());
        headers.insert(
            "X-Chatos-Sandbox-Lease-Id".to_string(),
            self.lease_id.clone(),
        );
        if let (Some(client_id), Some(client_key)) = (
            self.manager_client_id.as_deref(),
            self.manager_client_key.as_deref(),
        ) {
            headers.insert("x-sandbox-client-id".to_string(), client_id.to_string());
            headers.insert("x-sandbox-client-key".to_string(), client_key.to_string());
        }
        headers.insert("X-Task-Runner-Task-Id".to_string(), task.id.clone());
        headers.insert("X-Task-Runner-Run-Id".to_string(), run.id.clone());
        headers.insert(
            "X-Task-Runner-Tenant-Id".to_string(),
            task.tenant_id.clone(),
        );
        headers.insert("X-Task-Runner-User-Id".to_string(), task.subject_id.clone());
        headers.insert(
            "X-Task-Runner-Project-Id".to_string(),
            task.project_id.clone(),
        );
        McpHttpServer::new(SANDBOX_MCP_SERVER_NAME, self.mcp_url.clone()).with_headers(headers)
    }
}

impl RunService {
    pub(super) async fn should_route_task_to_sandbox(
        &self,
        task: &TaskRecord,
    ) -> Result<bool, String> {
        if !task.mcp_config.enabled {
            return Ok(false);
        }
        if let Some(enabled) = task.mcp_config.sandbox_enabled {
            return Ok(enabled);
        }
        let sandbox_enabled = self.effective_sandbox_enabled().await?;
        Ok(sandbox_enabled && task_requires_sandbox(task))
    }

    pub(super) async fn prepare_sandbox_if_needed(
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

    async fn sandbox_manager_base_url_for_task(&self, task: &TaskRecord) -> Result<String, String> {
        if let Some(base_url) = task
            .mcp_config
            .sandbox_manager_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(base_url.trim_end_matches('/').to_string());
        }
        self.effective_sandbox_manager_base_url().await
    }

    pub(super) async fn release_sandbox(
        &self,
        run: &TaskRunRecord,
        context: &SandboxRuntimeContext,
    ) -> Option<SandboxOutputReport> {
        let base_url = if context.manager_base_url.trim().is_empty() {
            match self.effective_sandbox_manager_base_url().await {
                Ok(base_url) => base_url,
                Err(err) => {
                    warn!(
                        run_id = run.id.as_str(),
                        sandbox_id = context.sandbox_id.as_str(),
                        "failed to load sandbox manager base url for release: {err}"
                    );
                    return None;
                }
            }
        } else {
            context.manager_base_url.clone()
        };
        let client = match SandboxManagerClient::new(
            base_url,
            SandboxManagerAuth::from_config(&self.config),
        ) {
            Ok(client) => client,
            Err(err) => {
                warn!(
                    run_id = run.id.as_str(),
                    sandbox_id = context.sandbox_id.as_str(),
                    "invalid sandbox manager base url for release: {err}"
                );
                return None;
            }
        };
        match client.release(context, true, true).await {
            Ok(response) => {
                let output_report = SandboxOutputReport::from_release_response(context, &response);
                let output_error = response.output_error.clone();
                let payload = json!({
                    "sandbox": context.to_metadata(),
                    "release": {
                        "ok": response.ok,
                        "status": response.status,
                        "output_workspace": response.output_workspace,
                        "diff_summary": response.diff_summary,
                        "output_error": output_error,
                        "change_counts": output_report.as_ref().map(|output| &output.file_change_counts),
                        "change_manifest_path": output_report.as_ref().and_then(|output| output.change_manifest_path.as_deref()),
                    },
                });
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "sandbox_released",
                        Some("沙箱已释放".to_string()),
                        Some(payload),
                    ))
                    .await
                {
                    warn!(
                        "failed to append sandbox release event for run {}: {}",
                        run.id, err
                    );
                }
                if let Some(output) = output_report.as_ref() {
                    if let Err(err) = self
                        .store
                        .append_run_event(TaskRunEventRecord::new(
                            run.id.clone(),
                            "sandbox_output_collected",
                            Some("沙箱输出变更清单已生成".to_string()),
                            Some(json!({
                                "sandbox": context.to_metadata(),
                                "output": output,
                            })),
                        ))
                        .await
                    {
                        warn!(
                            "failed to append sandbox output event for run {}: {}",
                            run.id, err
                        );
                    }
                }
                if let Some(output_error) = response
                    .output_error
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    if let Err(err) = self
                        .store
                        .append_run_event(TaskRunEventRecord::new(
                            run.id.clone(),
                            "sandbox_output_collect_failed",
                            Some(format!("沙箱输出变更清单生成失败: {output_error}")),
                            Some(json!({
                                "sandbox": context.to_metadata(),
                                "error": output_error,
                            })),
                        ))
                        .await
                    {
                        warn!(
                            "failed to append sandbox output failure event for run {}: {}",
                            run.id, err
                        );
                    }
                }
                output_report
            }
            Err(err) => {
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "sandbox_release_failed",
                        Some(format!("释放沙箱失败: {err}")),
                        Some(context.to_metadata()),
                    ))
                    .await
                {
                    warn!(
                        "failed to append sandbox release failure event for run {}: {}",
                        run.id, event_err
                    );
                }
                warn!(
                    run_id = run.id.as_str(),
                    sandbox_id = context.sandbox_id.as_str(),
                    "failed to release sandbox: {err}"
                );
                None
            }
        }
    }

    pub async fn get_run_output_changes(
        &self,
        run_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Option<RunOutputChangesResponse>, String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        let Some(manifest) = read_output_change_manifest_for_run(&run)? else {
            return Ok(Some(RunOutputChangesResponse {
                run_id: run.id,
                counts: RunOutputFileChangeCounts::default(),
                files: Vec::new(),
                total: 0,
                limit: limit.unwrap_or(100).clamp(1, 500),
                offset: offset.unwrap_or(0),
                has_more: false,
            }));
        };
        let total = manifest.files.len();
        let limit = limit.unwrap_or(100).clamp(1, 500);
        let offset = offset.unwrap_or(0);
        let files = manifest
            .files
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        Ok(Some(RunOutputChangesResponse {
            run_id: run.id,
            counts: manifest.counts,
            files,
            total,
            limit,
            offset,
            has_more: offset.saturating_add(limit) < total,
        }))
    }

    pub async fn get_run_output_diff(
        &self,
        run_id: &str,
        path: &str,
    ) -> Result<Option<RunOutputDiffResponse>, String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        let Some(manifest) = read_output_change_manifest_for_run(&run)? else {
            return Ok(Some(RunOutputDiffResponse {
                run_id: run.id,
                path: normalize_output_relative_path(path)?,
                status: "unknown".to_string(),
                patch: None,
                binary: false,
                diff_available: false,
                diff_truncated: false,
                message: Some("本次运行没有文件变更清单。".to_string()),
            }));
        };
        let normalized_path = normalize_output_relative_path(path)?;
        let Some(change) = manifest
            .files
            .iter()
            .find(|file| file.path == normalized_path)
        else {
            return Err("文件不在本次运行变更清单中".to_string());
        };
        let patch = if change.diff_available {
            Some(read_output_diff_file(&manifest, change)?)
        } else {
            None
        };
        let message = if change.diff_available {
            None
        } else if change.binary {
            Some("该文件是二进制文件或包含非文本内容，未生成 diff 预览。".to_string())
        } else {
            Some("该文件没有可用 diff 预览。".to_string())
        };
        Ok(Some(RunOutputDiffResponse {
            run_id: run.id,
            path: change.path.clone(),
            status: change.status.clone(),
            patch,
            binary: change.binary,
            diff_available: change.diff_available,
            diff_truncated: change.diff_truncated,
            message,
        }))
    }

    async fn append_sandbox_event(
        &self,
        run: &TaskRunRecord,
        event_type: &str,
        message: impl Into<String>,
        payload: Option<Value>,
    ) {
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                event_type.to_string(),
                Some(message.into()),
                payload,
            ))
            .await
        {
            warn!(
                "failed to append sandbox event {} for run {}: {}",
                event_type, run.id, err
            );
        }
    }
}

impl SandboxRuntimeContext {
    fn from_response(
        response: CreateSandboxLeaseResponse,
        workspace_root: &Path,
        manager_base_url: &str,
        manager_auth: Option<SandboxManagerAuth>,
    ) -> Result<Self, String> {
        let agent_endpoint = response
            .agent_endpoint
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "sandbox agent endpoint is empty".to_string())?;
        let manager_base_url = manager_base_url.trim().trim_end_matches('/').to_string();
        if manager_base_url.is_empty() {
            return Err("sandbox manager base url is empty".to_string());
        }
        let lease_id = response.lease_id;
        let sandbox_id = response.sandbox_id;
        let agent_token = response
            .agent_token
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| lease_id.clone());
        let (manager_client_id, manager_client_key) = manager_auth
            .map(|auth| (Some(auth.client_id), Some(auth.client_key)))
            .unwrap_or((None, None));
        Ok(Self {
            lease_id,
            sandbox_id: sandbox_id.clone(),
            backend_id: response.backend_id,
            agent_token,
            mcp_url: format!("{manager_base_url}/api/sandboxes/{sandbox_id}/mcp"),
            manager_client_id,
            manager_client_key,
            manager_base_url,
            agent_endpoint,
            run_workspace: response.run_workspace,
            workspace_root: workspace_root.to_string_lossy().to_string(),
            expires_at: response.expires_at,
        })
    }
}

pub(super) fn task_requires_sandbox(task: &TaskRecord) -> bool {
    if !task.mcp_config.enabled {
        return false;
    }
    runtime_selected_builtin_kinds(task)
        .into_iter()
        .any(|kind| {
            matches!(
                kind,
                BuiltinMcpKind::CodeMaintainerWrite | BuiltinMcpKind::TerminalController
            )
        })
}

pub(super) fn sandbox_replaces_builtin_kind(kind: BuiltinMcpKind) -> bool {
    matches!(
        kind,
        BuiltinMcpKind::CodeMaintainerRead
            | BuiltinMcpKind::CodeMaintainerWrite
            | BuiltinMcpKind::TerminalController
    )
}

fn attach_sandbox_context_to_run(run: &mut TaskRunRecord, context: &SandboxRuntimeContext) {
    if let Some(object) = run.input_snapshot.as_object_mut() {
        object.insert("sandbox_enabled".to_string(), Value::Bool(true));
        object.insert("sandbox".to_string(), context.to_metadata());
    }
}
