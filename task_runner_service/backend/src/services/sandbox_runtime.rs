// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chatos_mcp_runtime::{BuiltinMcpKind, McpHttpServer};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::models::{
    RunOutputChangeManifest, RunOutputChangesResponse, RunOutputDiffResponse, RunOutputFileChange,
    RunOutputFileChangeCounts,
};

use super::workspace_mcp::runtime_selected_builtin_kinds;
use super::*;

pub(super) const SANDBOX_MCP_SERVER_NAME: &str = "sandbox";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SandboxOutputReport {
    pub enabled: bool,
    pub sandbox_id: String,
    pub lease_id: String,
    #[serde(default)]
    pub output_workspace: Option<String>,
    #[serde(default)]
    pub change_manifest_path: Option<String>,
    #[serde(default)]
    pub file_change_counts: RunOutputFileChangeCounts,
    #[serde(default)]
    pub file_changes_preview: Vec<RunOutputFileChange>,
    #[serde(default)]
    pub truncated: bool,
}

impl SandboxOutputReport {
    fn from_release_response(
        context: &SandboxRuntimeContext,
        response: &ReleaseSandboxResponse,
    ) -> Option<Self> {
        let manifest = response.change_manifest.as_ref()?;
        let preview_limit = 20usize;
        Some(Self {
            enabled: true,
            sandbox_id: context.sandbox_id.clone(),
            lease_id: context.lease_id.clone(),
            output_workspace: response.output_workspace.clone(),
            change_manifest_path: manifest.manifest_path.clone(),
            file_change_counts: manifest.counts.clone(),
            file_changes_preview: manifest.files.iter().take(preview_limit).cloned().collect(),
            truncated: manifest.files.len() > preview_limit,
        })
    }
}

fn read_output_change_manifest_for_run(
    run: &TaskRunRecord,
) -> Result<Option<RunOutputChangeManifest>, String> {
    let Some(output) = sandbox_output_report_from_run(run)? else {
        return Ok(None);
    };
    let Some(manifest_path) = output
        .change_manifest_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let text =
        fs::read_to_string(manifest_path).map_err(|err| format!("读取沙箱变更清单失败: {err}"))?;
    let manifest = serde_json::from_str::<RunOutputChangeManifest>(&text)
        .map_err(|err| format!("解析沙箱变更清单失败: {err}"))?;
    if manifest.run_id != run.id {
        return Err("沙箱变更清单与运行 ID 不匹配".to_string());
    }
    Ok(Some(manifest))
}

fn sandbox_output_report_from_run(
    run: &TaskRunRecord,
) -> Result<Option<SandboxOutputReport>, String> {
    let Some(report) = run.report.as_ref() else {
        return Ok(None);
    };
    let Some(output) = report.pointer("/output/sandbox") else {
        return Ok(None);
    };
    serde_json::from_value::<SandboxOutputReport>(output.clone())
        .map(Some)
        .map_err(|err| format!("解析沙箱输出摘要失败: {err}"))
}

fn read_output_diff_file(
    manifest: &RunOutputChangeManifest,
    change: &RunOutputFileChange,
) -> Result<String, String> {
    let diff_ref = change
        .diff_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "该文件没有 diff 引用".to_string())?;
    let manifest_path = manifest
        .manifest_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "变更清单缺少 manifest_path".to_string())?;
    let manifest_dir = Path::new(manifest_path)
        .parent()
        .ok_or_else(|| "变更清单路径无效".to_string())?;
    let safe_ref = normalize_output_relative_path(diff_ref)?;
    let candidate = manifest_dir.join(safe_ref);
    ensure_child_path(manifest_dir, candidate.as_path())?;
    fs::read_to_string(candidate.as_path()).map_err(|err| format!("读取 diff 文件失败: {err}"))
}

fn normalize_output_relative_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    let path = Path::new(trimmed.as_str());
    if path.is_absolute() {
        return Err("文件路径不能是绝对路径".to_string());
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => return Err("文件路径不能包含 ..".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("文件路径不能是绝对路径".to_string());
            }
        }
    }
    if parts.is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    Ok(parts.join("/"))
}

fn ensure_child_path(root: &Path, candidate: &Path) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|err| format!("读取 diff 根目录失败: {err}"))?;
    let candidate =
        fs::canonicalize(candidate).map_err(|err| format!("读取 diff 路径失败: {err}"))?;
    if candidate.starts_with(root.as_path()) {
        Ok(())
    } else {
        Err("diff 路径越界".to_string())
    }
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

fn sandbox_workspace_root(workspace_dir: &str) -> Result<PathBuf, String> {
    let root = Path::new(workspace_dir).join(".chatos").join("task-runner");
    fs::create_dir_all(&root).map_err(|err| {
        format!(
            "create sandbox workspace root {} failed: {err}",
            root.display()
        )
    })?;
    Ok(root)
}

fn is_local_connector_sandbox_manager(base_url: &str) -> bool {
    base_url.contains("/api/local-connectors/sandbox-facade/")
}

fn sandbox_baseline_workspace(run_workspace: &str) -> Result<String, String> {
    let run_workspace = Path::new(run_workspace);
    let run_root = run_workspace
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "invalid sandbox run workspace path".to_string())?;
    Ok(run_root
        .join("baseline")
        .join("workspace")
        .to_string_lossy()
        .to_string())
}

fn copy_workspace_to_sandbox(source: &str, destination: &str) -> Result<(), String> {
    let source = fs::canonicalize(source)
        .map_err(|err| format!("read source workspace {source} failed: {err}"))?;
    let destination = PathBuf::from(destination);
    fs::create_dir_all(&destination).map_err(|err| {
        format!(
            "create sandbox run workspace {} failed: {err}",
            destination.display()
        )
    })?;
    clear_directory(destination.as_path())?;
    copy_directory_contents(source.as_path(), destination.as_path(), source.as_path())
}

fn clear_directory(path: &Path) -> Result<(), String> {
    for entry in fs::read_dir(path)
        .map_err(|err| format!("read directory {} failed: {err}", path.display()))?
    {
        let entry = entry.map_err(|err| format!("read directory entry failed: {err}"))?;
        let entry_path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| format!("read metadata {} failed: {err}", entry_path.display()))?;
        if metadata.is_dir() {
            fs::remove_dir_all(&entry_path).map_err(|err| {
                format!("remove directory {} failed: {err}", entry_path.display())
            })?;
        } else {
            fs::remove_file(&entry_path)
                .map_err(|err| format!("remove file {} failed: {err}", entry_path.display()))?;
        }
    }
    Ok(())
}

fn copy_directory_contents(source: &Path, destination: &Path, root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source)
        .map_err(|err| format!("read directory {} failed: {err}", source.display()))?
    {
        let entry = entry.map_err(|err| format!("read directory entry failed: {err}"))?;
        let source_path = entry.path();
        if should_skip_workspace_entry(root, source_path.as_path()) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|err| format!("read file type {} failed: {err}", source_path.display()))?;
        let dest_path = destination.join(entry.file_name());
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            fs::create_dir_all(&dest_path)
                .map_err(|err| format!("create directory {} failed: {err}", dest_path.display()))?;
            copy_directory_contents(source_path.as_path(), dest_path.as_path(), root)?;
        } else if file_type.is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    format!("create directory {} failed: {err}", parent.display())
                })?;
            }
            fs::copy(&source_path, &dest_path).map_err(|err| {
                format!(
                    "copy file {} to {} failed: {err}",
                    source_path.display(),
                    dest_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn should_skip_workspace_entry(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    relative
        .components()
        .next()
        .is_some_and(|component| matches!(component, Component::Normal(name) if name == ".chatos"))
}

#[derive(Debug, Serialize)]
struct CreateSandboxLeaseRequest {
    tenant_id: String,
    user_id: String,
    project_id: String,
    run_id: String,
    workspace_root: String,
    tools: Vec<String>,
    ttl_seconds: u64,
}

#[derive(Debug, Deserialize)]
struct CreateSandboxLeaseResponse {
    lease_id: String,
    sandbox_id: String,
    backend_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    agent_endpoint: Option<String>,
    agent_token: Option<String>,
    run_workspace: String,
    expires_at: String,
    #[serde(default)]
    last_error: Option<String>,
}

impl CreateSandboxLeaseResponse {
    fn status_label(&self) -> &str {
        self.status.as_deref().unwrap_or("unknown")
    }

    fn is_ready(&self) -> bool {
        matches!(
            self.status.as_deref().unwrap_or("ready"),
            "ready" | "running"
        ) && self
            .agent_endpoint
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    }

    fn is_waiting(&self) -> bool {
        if self.status.is_none() {
            return self
                .agent_endpoint
                .as_deref()
                .map(str::trim)
                .map_or(true, str::is_empty);
        }
        matches!(
            self.status.as_deref().unwrap_or("leasing"),
            "pending" | "leasing" | "starting"
        )
    }

    fn is_terminal_failure(&self) -> bool {
        matches!(
            self.status.as_deref(),
            Some("failed" | "expired" | "destroyed")
        )
    }

    fn apply_record(&mut self, record: SandboxLeaseRecordResponse) {
        self.backend_id = record.backend_id;
        self.status = Some(record.status);
        self.agent_endpoint = record.agent_endpoint;
        self.run_workspace = record.run_workspace;
        self.expires_at = record.expires_at;
        self.last_error = record.last_error;
    }
}

#[derive(Debug, Deserialize)]
struct SandboxLeaseRecordResponse {
    backend_id: Option<String>,
    status: String,
    agent_endpoint: Option<String>,
    run_workspace: String,
    expires_at: String,
    last_error: Option<String>,
}

fn sandbox_wait_deadline(expires_at: &str) -> tokio::time::Instant {
    let fallback = tokio::time::Instant::now() + Duration::from_secs(7_200);
    let Ok(expires_at) = DateTime::parse_from_rfc3339(expires_at) else {
        return fallback;
    };
    let remaining = expires_at
        .with_timezone(&Utc)
        .signed_duration_since(Utc::now());
    if remaining <= chrono::Duration::zero() {
        return tokio::time::Instant::now();
    }
    tokio::time::Instant::now()
        + remaining.to_std().unwrap_or(Duration::from_secs(7_200))
        + Duration::from_secs(30)
}

#[derive(Debug, Serialize)]
struct ReleaseSandboxRequest {
    lease_id: String,
    export_result: bool,
    destroy: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReleaseSandboxResponse {
    ok: bool,
    status: String,
    output_workspace: Option<String>,
    diff_summary: Option<String>,
    output_error: Option<String>,
    change_manifest: Option<RunOutputChangeManifest>,
}

struct SandboxHealthResult {
    ok: bool,
    message: String,
    raw: Value,
}

struct SandboxManagerClient {
    base_url: String,
    client: reqwest::Client,
    auth: Option<SandboxManagerAuth>,
}

#[derive(Debug, Clone)]
struct SandboxManagerAuth {
    client_id: String,
    client_key: String,
}

impl SandboxManagerClient {
    fn new(base_url: String, auth: Option<SandboxManagerAuth>) -> Result<Self, String> {
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err("sandbox manager base url is empty".to_string());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|err| format!("build sandbox manager http client failed: {err}"))?;
        Ok(Self {
            base_url,
            client,
            auth,
        })
    }

    async fn create_lease(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_root: &Path,
        ttl_seconds: u64,
    ) -> Result<CreateSandboxLeaseResponse, String> {
        let payload = CreateSandboxLeaseRequest {
            tenant_id: task.tenant_id.clone(),
            user_id: task.subject_id.clone(),
            project_id: task.project_id.clone(),
            run_id: run.id.clone(),
            workspace_root: workspace_root.to_string_lossy().to_string(),
            tools: vec!["filesystem".to_string(), "terminal".to_string()],
            ttl_seconds,
        };
        let idempotency_key = format!("sandbox-lease:{}", run.id);
        let url = format!("{}/api/sandboxes/leases", self.base_url);
        for attempt in 0..6 {
            let response = self
                .apply_auth(self.client.post(url.as_str()))
                .header("x-idempotency-key", idempotency_key.as_str())
                .json(&payload)
                .send()
                .await
                .map_err(|err| format!("request sandbox lease failed: {err}"))?;
            let status = response.status();
            if status == reqwest::StatusCode::CONFLICT {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|err| format!("read conflict body failed: {err}"));
                if body.contains("sandbox_lease_idempotency_in_progress") && attempt < 5 {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                return Err(format!(
                    "sandbox lease request returned HTTP {status}: {body}"
                ));
            }
            return response
                .error_for_status()
                .map_err(|err| format!("sandbox lease request returned error: {err}"))?
                .json::<CreateSandboxLeaseResponse>()
                .await
                .map_err(|err| format!("decode sandbox lease response failed: {err}"));
        }
        Err("sandbox lease idempotency retry loop exhausted".to_string())
    }

    async fn wait_until_ready(
        &self,
        mut response: CreateSandboxLeaseResponse,
    ) -> Result<CreateSandboxLeaseResponse, String> {
        let mut deadline = sandbox_wait_deadline(response.expires_at.as_str());
        loop {
            if response.is_ready() {
                return Ok(response);
            }
            if response.is_terminal_failure() {
                let detail = response
                    .last_error
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("no error detail");
                return Err(format!(
                    "sandbox lease reached terminal status {}: {detail}",
                    response.status_label()
                ));
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "sandbox lease did not become ready before timeout: sandbox_id={}, lease_id={}, status={}",
                    response.sandbox_id,
                    response.lease_id,
                    response.status_label()
                ));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            let record = self.get_sandbox(response.sandbox_id.as_str()).await?;
            response.apply_record(record);
            deadline = sandbox_wait_deadline(response.expires_at.as_str());
        }
    }

    async fn get_sandbox(&self, sandbox_id: &str) -> Result<SandboxLeaseRecordResponse, String> {
        self.apply_auth(
            self.client
                .get(format!("{}/api/sandboxes/{}", self.base_url, sandbox_id)),
        )
        .send()
        .await
        .map_err(|err| format!("request sandbox detail failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("sandbox detail request returned error: {err}"))?
        .json::<SandboxLeaseRecordResponse>()
        .await
        .map_err(|err| format!("decode sandbox detail response failed: {err}"))
    }

    async fn health(&self, context: &SandboxRuntimeContext) -> Result<SandboxHealthResult, String> {
        let raw = self
            .apply_auth(self.client.get(format!(
                "{}/api/sandboxes/{}/health",
                self.base_url, context.sandbox_id
            )))
            .send()
            .await
            .map_err(|err| format!("request sandbox health failed: {err}"))?
            .error_for_status()
            .map_err(|err| format!("sandbox health request returned error: {err}"))?
            .json::<Value>()
            .await
            .map_err(|err| format!("decode sandbox health response failed: {err}"))?;
        let ok = raw.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let message = raw
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or(if ok { "ok" } else { "unknown health failure" })
            .to_string();
        Ok(SandboxHealthResult { ok, message, raw })
    }

    async fn release(
        &self,
        context: &SandboxRuntimeContext,
        export_result: bool,
        destroy: bool,
    ) -> Result<ReleaseSandboxResponse, String> {
        let payload = ReleaseSandboxRequest {
            lease_id: context.lease_id.clone(),
            export_result,
            destroy,
        };
        self.apply_auth(self.client.post(format!(
            "{}/api/sandboxes/{}/release",
            self.base_url, context.sandbox_id
        )))
        .json(&payload)
        .send()
        .await
        .map_err(|err| format!("request sandbox release failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("sandbox release request returned error: {err}"))?
        .json::<ReleaseSandboxResponse>()
        .await
        .map_err(|err| format!("decode sandbox release response failed: {err}"))
    }

    fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(auth) = self.auth.as_ref() {
            request
                .header("x-sandbox-client-id", auth.client_id.as_str())
                .header("x-sandbox-client-key", auth.client_key.as_str())
        } else {
            request
        }
    }
}

impl SandboxManagerAuth {
    fn from_config(config: &AppConfig) -> Option<Self> {
        match (
            config.sandbox_manager_client_id.clone(),
            config.sandbox_manager_client_key.clone(),
        ) {
            (Some(client_id), Some(client_key)) => Some(Self {
                client_id,
                client_key,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_output_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "chatos-task-runner-output-{name}-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).expect("create temp output dir");
        path
    }

    fn manifest_at(path: &Path) -> RunOutputChangeManifest {
        RunOutputChangeManifest {
            schema_version: 1,
            run_id: "run-1".to_string(),
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            output_workspace: None,
            manifest_path: Some(path.to_string_lossy().to_string()),
            counts: RunOutputFileChangeCounts::default(),
            files: Vec::new(),
        }
    }

    fn change_with_diff_ref(diff_ref: &str) -> RunOutputFileChange {
        RunOutputFileChange {
            path: "src/main.rs".to_string(),
            status: "modified".to_string(),
            old_size: None,
            new_size: None,
            old_sha256: None,
            new_sha256: None,
            added_lines: 1,
            deleted_lines: 1,
            binary: false,
            diff_available: true,
            diff_truncated: false,
            diff_ref: Some(diff_ref.to_string()),
        }
    }

    #[test]
    fn output_relative_path_rejects_absolute_and_parent_paths() {
        assert_eq!(
            normalize_output_relative_path("diffs/file.diff").expect("valid path"),
            "diffs/file.diff"
        );
        assert!(normalize_output_relative_path("../file.diff").is_err());
        assert!(normalize_output_relative_path("diffs/../file.diff").is_err());
        assert!(normalize_output_relative_path("/tmp/file.diff").is_err());
    }

    #[test]
    fn output_diff_reader_is_scoped_to_manifest_directory() {
        let output_root = temp_output_dir("diff-scope");
        let manifest_path = output_root.join("change_manifest.json");
        let diff_root = output_root.join("diffs");
        std::fs::create_dir_all(&diff_root).expect("create diff dir");
        std::fs::write(
            diff_root.join("main.diff"),
            "diff --git a/src/main.rs b/src/main.rs\n",
        )
        .expect("write diff");

        let manifest = manifest_at(manifest_path.as_path());
        let change = change_with_diff_ref("diffs/main.diff");
        let diff = read_output_diff_file(&manifest, &change).expect("read diff");
        assert!(diff.contains("diff --git"));

        let escaped = change_with_diff_ref("../outside.diff");
        assert!(read_output_diff_file(&manifest, &escaped).is_err());

        std::fs::remove_dir_all(output_root).expect("cleanup");
    }
}
