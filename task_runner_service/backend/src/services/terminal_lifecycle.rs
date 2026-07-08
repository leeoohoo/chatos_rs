// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_builtin_tools::TerminalControllerContext;
use chatos_mcp_runtime::BuiltinMcpKind;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{info, warn};

use crate::models::{
    TaskEphemeralHttpMcpServer, TaskRecord, TaskRunEventRecord, TaskRunRecord,
    TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL,
};
use crate::terminal_store::TaskRunnerTerminalControllerStore;

use super::workspace_mcp::{
    selected_builtin_kinds, task_uses_local_connector, task_uses_local_connector_builtin_kind,
};
use super::RunService;

impl RunService {
    pub(super) async fn ensure_task_terminal_started(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_dir: &str,
    ) {
        if !task_terminal_enabled(task) {
            return;
        }
        if task_uses_local_connector(task) {
            self.start_local_connector_task_terminal(task, run, workspace_dir)
                .await;
            return;
        }
        match self.should_route_task_to_sandbox(task).await {
            Ok(true) => {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner skipped local task terminal because sandbox routing is enabled"
                );
                return;
            }
            Ok(false) => {}
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner skipped local task terminal because sandbox routing config could not be loaded: {}",
                    err
                );
                return;
            }
        }
        let context = self.task_terminal_context(task, workspace_dir);
        match TaskRunnerTerminalControllerStore
            .start_shell_session(context, ".".to_string())
            .await
        {
            Ok(payload) => {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner started initial task terminal"
                );
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_started",
                        Some("已创建任务终端".to_string()),
                        Some(payload),
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_started event for run {}: {}",
                        run.id, err
                    );
                }
            }
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "failed to start initial task terminal: {}",
                    err
                );
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_start_failed",
                        Some(format!("创建任务终端失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_start_failed event for run {}: {}",
                        run.id, event_err
                    );
                }
            }
        }
    }

    pub(super) async fn cleanup_task_terminals(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_dir: &str,
    ) {
        if !task_terminal_enabled(task) {
            return;
        }
        if task_uses_local_connector(task) {
            self.cleanup_local_connector_task_terminals(task, run, workspace_dir)
                .await;
            return;
        }
        let context = self.task_terminal_context(task, workspace_dir);
        match TaskRunnerTerminalControllerStore
            .kill_sessions_for_context(context)
            .await
        {
            Ok(payload) => {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner cleaned up task terminals"
                );
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_cleanup",
                        Some("已关闭本次任务终端".to_string()),
                        Some(payload),
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_cleanup event for run {}: {}",
                        run.id, err
                    );
                }
            }
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "failed to clean up task terminals: {}",
                    err
                );
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_cleanup_failed",
                        Some(format!("关闭任务终端失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_cleanup_failed event for run {}: {}",
                        run.id, event_err
                    );
                }
            }
        }
    }

    fn task_terminal_context(
        &self,
        task: &TaskRecord,
        workspace_dir: &str,
    ) -> TerminalControllerContext {
        TerminalControllerContext {
            root: workspace_dir.into(),
            user_id: Some(task.subject_id.clone()),
            project_id: Some(task.id.clone()),
            idle_timeout_ms: 5_000,
            max_wait_ms: 60_000,
            max_output_chars: 20_000,
        }
    }

    async fn start_local_connector_task_terminal(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_dir: &str,
    ) {
        match self
            .call_local_connector_terminal_lifecycle(
                task,
                run,
                "local_connector/terminal/start",
                json!({ "path": "." }),
            )
            .await
        {
            Ok(payload) => {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner started Local Connector task terminal"
                );
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_started",
                        Some("已创建 Local Connector 任务终端".to_string()),
                        Some(payload),
                    ))
                    .await
                {
                    warn!(
                        "failed to append local connector terminal_started event for run {}: {}",
                        run.id, err
                    );
                }
            }
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "failed to start Local Connector task terminal: {}",
                    err
                );
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_start_failed",
                        Some(format!("创建 Local Connector 任务终端失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append local connector terminal_start_failed event for run {}: {}",
                        run.id, event_err
                    );
                }
            }
        }
    }

    async fn cleanup_local_connector_task_terminals(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_dir: &str,
    ) {
        match self
            .call_local_connector_terminal_lifecycle(
                task,
                run,
                "local_connector/terminal/cleanup",
                json!({}),
            )
            .await
        {
            Ok(payload) => {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner cleaned up Local Connector task terminals"
                );
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_cleanup",
                        Some("已关闭本次 Local Connector 任务终端".to_string()),
                        Some(payload),
                    ))
                    .await
                {
                    warn!(
                        "failed to append local connector terminal_cleanup event for run {}: {}",
                        run.id, err
                    );
                }
            }
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "failed to clean up Local Connector task terminals: {}",
                    err
                );
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_cleanup_failed",
                        Some(format!("关闭 Local Connector 任务终端失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append local connector terminal_cleanup_failed event for run {}: {}",
                        run.id, event_err
                    );
                }
            }
        }
    }

    async fn call_local_connector_terminal_lifecycle(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let server = local_connector_ephemeral_server(task)
            .ok_or_else(|| "Local Connector MCP server is not configured".to_string())?;
        let headers = self.local_connector_lifecycle_headers(task, run, server)?;
        let response = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| err.to_string())?
            .post(server.url.as_str())
            .headers(headers)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": format!("{}:{}", run.id, method),
                "method": method,
                "params": params,
            }))
            .send()
            .await
            .map_err(|err| err.to_string())?;
        let status = response.status();
        let body = response.text().await.map_err(|err| err.to_string())?;
        if !status.is_success() {
            return Err(format!("Local Connector lifecycle HTTP {status}: {body}"));
        }
        let value = serde_json::from_str::<Value>(body.as_str()).map_err(|err| {
            format!("Local Connector lifecycle response parse failed: {err}; body={body}")
        })?;
        if let Some(error) = value.get("error") {
            return Err(format!("Local Connector lifecycle error: {error}"));
        }
        Ok(value.get("result").cloned().unwrap_or(value))
    }

    fn local_connector_lifecycle_headers(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        server: &TaskEphemeralHttpMcpServer,
    ) -> Result<HeaderMap, String> {
        let mut headers = HeaderMap::new();
        for (key, value) in &server.headers {
            let name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|err| format!("invalid Local Connector header name {key}: {err}"))?;
            let value = HeaderValue::from_str(value.as_str())
                .map_err(|err| format!("invalid Local Connector header value {key}: {err}"))?;
            headers.insert(name, value);
        }
        if server.auth_mode.as_deref() == Some(TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL) {
            let secret = self
                .config
                .local_connector_internal_api_secret
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required for Local Connector lifecycle".to_string())?;
            let owner_user_id = task_owner_user_id(task).ok_or_else(|| {
                "Local Connector lifecycle requires task owner user id".to_string()
            })?;
            headers.insert(
                HeaderName::from_static("x-local-connector-internal-secret"),
                HeaderValue::from_str(secret).map_err(|err| err.to_string())?,
            );
            headers.insert(
                HeaderName::from_static("x-local-connector-owner-user-id"),
                HeaderValue::from_str(owner_user_id.as_str()).map_err(|err| err.to_string())?,
            );
        }
        headers.insert(
            HeaderName::from_static("x-task-runner-task-id"),
            HeaderValue::from_str(task.id.as_str()).map_err(|err| err.to_string())?,
        );
        headers.insert(
            HeaderName::from_static("x-task-runner-run-id"),
            HeaderValue::from_str(run.id.as_str()).map_err(|err| err.to_string())?,
        );
        Ok(headers)
    }
}

fn task_terminal_enabled(task: &TaskRecord) -> bool {
    if !task.mcp_config.enabled {
        return false;
    }
    if task_uses_local_connector(task) {
        return task_uses_local_connector_builtin_kind(task, BuiltinMcpKind::TerminalController);
    }
    selected_builtin_kinds(&task.mcp_config)
        .into_iter()
        .any(|kind| kind == BuiltinMcpKind::TerminalController)
}

fn local_connector_ephemeral_server(task: &TaskRecord) -> Option<&TaskEphemeralHttpMcpServer> {
    task.mcp_config
        .ephemeral_http_servers
        .iter()
        .find(|server| {
            server
                .auth_mode
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| {
                    value.eq_ignore_ascii_case(TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL)
                })
                || server.name.trim().eq_ignore_ascii_case("local_connector")
        })
}

fn task_owner_user_id(task: &TaskRecord) -> Option<String> {
    task.owner_user_id
        .as_deref()
        .or(task.creator_user_id.as_deref())
        .or(Some(task.subject_id.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
