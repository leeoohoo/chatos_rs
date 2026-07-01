use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chatos_mcp_runtime::{BuiltinMcpKind, BuiltinMcpPromptLocale, McpHttpServer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use super::workspace_mcp::runtime_selected_builtin_kinds;
use super::*;

pub(super) const SANDBOX_MCP_SERVER_NAME: &str = "sandbox";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SandboxRuntimeContext {
    pub lease_id: String,
    pub sandbox_id: String,
    pub backend_id: Option<String>,
    pub agent_endpoint: String,
    pub mcp_url: String,
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
        let base_url = self.effective_sandbox_manager_base_url().await?;
        let ttl_seconds = self.effective_sandbox_lease_ttl_seconds().await?;
        let client = SandboxManagerClient::new(base_url)?;

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

        let context = match SandboxRuntimeContext::from_response(response, workspace_root.as_path())
        {
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

        if let Err(err) = copy_workspace_to_sandbox(effective_workspace_dir, &context.run_workspace)
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
            Some(context.to_metadata()),
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

    pub(super) async fn release_sandbox(
        &self,
        run: &TaskRunRecord,
        context: &SandboxRuntimeContext,
    ) {
        let base_url = match self.effective_sandbox_manager_base_url().await {
            Ok(base_url) => base_url,
            Err(err) => {
                warn!(
                    run_id = run.id.as_str(),
                    sandbox_id = context.sandbox_id.as_str(),
                    "failed to load sandbox manager base url for release: {err}"
                );
                return;
            }
        };
        let client = match SandboxManagerClient::new(base_url) {
            Ok(client) => client,
            Err(err) => {
                warn!(
                    run_id = run.id.as_str(),
                    sandbox_id = context.sandbox_id.as_str(),
                    "invalid sandbox manager base url for release: {err}"
                );
                return;
            }
        };
        match client.release(context, true, true).await {
            Ok(response) => {
                let payload = json!({
                    "sandbox": context.to_metadata(),
                    "release": response,
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
            }
        }
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
    ) -> Result<Self, String> {
        let agent_endpoint = response
            .agent_endpoint
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "sandbox agent endpoint is empty".to_string())?;
        Ok(Self {
            lease_id: response.lease_id,
            sandbox_id: response.sandbox_id,
            backend_id: response.backend_id,
            mcp_url: format!("{agent_endpoint}/mcp"),
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

pub(super) fn sandbox_mcp_prefixed_input_items(
    context: &SandboxRuntimeContext,
    locale: BuiltinMcpPromptLocale,
) -> Vec<Value> {
    let text = if locale.is_english() {
        format!(
            "[Sandbox]\nTask Runner created an isolated sandbox for this run. File and terminal operations must use the `sandbox_*` MCP tools exposed by the sandbox service. Treat paths as relative to the sandbox workspace unless a tool asks otherwise. Sandbox id: `{}`. Lease id: `{}`. Host-side run workspace copy: `{}`.",
            context.sandbox_id, context.lease_id, context.run_workspace
        )
    } else {
        format!(
            "[沙箱]\nTask Runner 已为本次运行创建隔离沙箱。文件读写和终端命令必须使用沙箱服务暴露的 `sandbox_*` MCP 工具。除非工具参数另有说明，路径都按沙箱工作区的相对路径处理。Sandbox ID：`{}`。Lease ID：`{}`。宿主机 `.chatos` 运行副本：`{}`。",
            context.sandbox_id, context.lease_id, context.run_workspace
        )
    };

    vec![json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": text
        }]
    })]
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
    agent_endpoint: Option<String>,
    run_workspace: String,
    expires_at: String,
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
}

struct SandboxHealthResult {
    ok: bool,
    message: String,
    raw: Value,
}

struct SandboxManagerClient {
    base_url: String,
    client: reqwest::Client,
}

impl SandboxManagerClient {
    fn new(base_url: String) -> Result<Self, String> {
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err("sandbox manager base url is empty".to_string());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|err| format!("build sandbox manager http client failed: {err}"))?;
        Ok(Self { base_url, client })
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
        self.client
            .post(format!("{}/api/sandboxes/leases", self.base_url))
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("request sandbox lease failed: {err}"))?
            .error_for_status()
            .map_err(|err| format!("sandbox lease request returned error: {err}"))?
            .json::<CreateSandboxLeaseResponse>()
            .await
            .map_err(|err| format!("decode sandbox lease response failed: {err}"))
    }

    async fn health(&self, context: &SandboxRuntimeContext) -> Result<SandboxHealthResult, String> {
        let raw = self
            .client
            .get(format!(
                "{}/api/sandboxes/{}/health",
                self.base_url, context.sandbox_id
            ))
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
        self.client
            .post(format!(
                "{}/api/sandboxes/{}/release",
                self.base_url, context.sandbox_id
            ))
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
}
