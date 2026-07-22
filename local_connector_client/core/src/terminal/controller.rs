// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chatos_mcp::{
    TerminalControllerContext, TerminalControllerOptions, TerminalControllerService,
    TerminalControllerStoreRef,
};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::{select_local_shell, MAX_TERMINAL_OUTPUT_BYTES};

mod output;
mod registry;
mod reused;
mod shell;
mod standalone;
mod store;

use registry::{
    append_local_mcp_terminal_log, local_mcp_sessions_for_context, local_mcp_terminal_registry,
    mark_local_mcp_terminal_exited, refresh_local_mcp_terminal_session_status,
    register_local_mcp_terminal_session, LocalMcpTerminalSession,
};
use reused::{
    execute_local_mcp_reused_shell_command, find_local_mcp_primary_shell_session,
    is_local_mcp_primary_shell_command, local_mcp_shell_session_is_busy,
};
use shell::{
    canonicalize_terminal_root, display_local_mcp_workspace_path, resolve_terminal_controller_cwd,
    shell_session_for_terminal_controller, terminate_terminal_process_tree,
};
use standalone::execute_local_mcp_standalone_command;

#[derive(Debug, Default)]
pub(crate) struct LocalConnectorTerminalControllerStore;

impl LocalConnectorTerminalControllerStore {
    pub(crate) async fn start_shell_session(
        &self,
        context: TerminalControllerContext,
        path: String,
    ) -> std::result::Result<Value, String> {
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let session = self.ensure_shell_session(context, path).await?;
        let meta = session.meta.lock().await.clone();
        let display_project_root =
            display_local_mcp_workspace_path(project_root.as_path(), project_root.as_path());
        let display_cwd =
            display_local_mcp_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
        Ok(json!({
            "project_root": display_project_root,
            "terminal_id": meta.id,
            "process_id": meta.id,
            "path": display_cwd,
            "command": meta.command,
            "background": true,
            "busy": meta.status != "exited",
            "status": meta.status,
            "started_at": meta.started_at,
            "project_id": meta.project_id,
            "user_id": meta.user_id,
        }))
    }

    async fn ensure_shell_session(
        &self,
        context: TerminalControllerContext,
        path: String,
    ) -> std::result::Result<Arc<LocalMcpTerminalSession>, String> {
        if let Some(session) = find_local_mcp_primary_shell_session(&context).await? {
            return Ok(session);
        }
        self.spawn_shell_session(context, path).await
    }

    async fn spawn_shell_session(
        &self,
        context: TerminalControllerContext,
        path: String,
    ) -> std::result::Result<Arc<LocalMcpTerminalSession>, String> {
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let cwd = resolve_terminal_controller_cwd(project_root.as_path(), path.as_str())?;
        let shell = select_local_shell();
        let mut child = shell_session_for_terminal_controller(shell.as_str());
        child
            .current_dir(cwd.as_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = child.spawn().map_err(|err| err.to_string())?;
        let session = register_local_mcp_terminal_session(
            context.clone(),
            project_root,
            cwd,
            format!("task terminal shell: {shell}"),
            child,
        )
        .await?;
        append_local_mcp_terminal_log(
            session.clone(),
            "system",
            "[task terminal shell started]\n".to_string(),
        )
        .await;
        Ok(session)
    }

    pub(crate) async fn kill_sessions_for_context(
        &self,
        context: TerminalControllerContext,
    ) -> std::result::Result<Value, String> {
        let sessions = local_mcp_sessions_for_context(&context).await?;
        let total = sessions.len();
        let mut killed = 0usize;
        let mut already_exited = 0usize;
        let mut errors = Vec::new();
        let mut terminal_ids = Vec::new();

        for session in sessions {
            if let Err(err) = refresh_local_mcp_terminal_session_status(&session).await {
                errors.push(err);
                continue;
            }
            let meta = session.meta.lock().await.clone();
            terminal_ids.push(meta.id.clone());
            if meta.status == "exited" {
                already_exited += 1;
                continue;
            }
            {
                let mut child = session.child.lock().await;
                if let Err(err) = terminate_terminal_process_tree(&mut child).await {
                    errors.push(format!("kill {} failed: {}", meta.id, err));
                    continue;
                }
            }
            mark_local_mcp_terminal_exited(&session, None).await;
            append_local_mcp_terminal_log(
                session.clone(),
                "system",
                "[task terminal cleanup killed process]\n".to_string(),
            )
            .await;
            killed += 1;
        }

        if !terminal_ids.is_empty() {
            let mut registry = local_mcp_terminal_registry().sessions.write().await;
            for terminal_id in &terminal_ids {
                registry.remove(terminal_id);
            }
        }

        Ok(json!({
            "ok": errors.is_empty(),
            "total": total,
            "killed": killed,
            "already_exited": already_exited,
            "terminal_ids": terminal_ids,
            "errors": errors,
        }))
    }
}

pub(crate) fn local_terminal_controller_context_for_root(
    root: &Path,
    request: &RelayRequest,
    timeout_ms: u64,
) -> TerminalControllerContext {
    TerminalControllerContext {
        root: root.to_path_buf(),
        user_id: request.owner_user_id.clone(),
        project_id: local_mcp_terminal_project_id(request),
        idle_timeout_ms: 1_000,
        max_wait_ms: timeout_ms,
        max_output_chars: MAX_TERMINAL_OUTPUT_BYTES,
    }
}

pub(crate) fn local_mcp_terminal_project_id(request: &RelayRequest) -> Option<String> {
    request
        .headers
        .get("x-task-runner-run-id")
        .or_else(|| request.headers.get("x-task-runner-task-id"))
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(request.workspace_id.clone()))
}

pub(crate) fn local_terminal_controller_context_for_task_run(
    root: &Path,
    owner_user_id: &str,
    run_id: &str,
    timeout_ms: u64,
) -> TerminalControllerContext {
    TerminalControllerContext {
        root: root.to_path_buf(),
        user_id: Some(owner_user_id.to_string()),
        project_id: Some(run_id.to_string()),
        idle_timeout_ms: 1_000,
        max_wait_ms: timeout_ms,
        max_output_chars: MAX_TERMINAL_OUTPUT_BYTES,
    }
}

pub(crate) async fn kill_local_terminal_sessions_for_task_run(
    run_id: &str,
) -> std::result::Result<Value, String> {
    let sessions = {
        let registry = local_mcp_terminal_registry().sessions.read().await;
        registry.values().cloned().collect::<Vec<_>>()
    };
    for session in sessions {
        let meta = session.meta.lock().await.clone();
        if meta.project_id.as_deref() != Some(run_id) {
            continue;
        }
        let context = local_terminal_controller_context_for_task_run(
            Path::new(meta.root.as_str()),
            meta.user_id.as_deref().unwrap_or_default(),
            run_id,
            30_000,
        );
        return LocalConnectorTerminalControllerStore
            .kill_sessions_for_context(context)
            .await;
    }
    Ok(json!({
        "ok": true,
        "total": 0,
        "killed": 0,
        "already_exited": 0,
        "terminal_ids": [],
        "errors": [],
    }))
}

pub(crate) fn local_terminal_controller_service_for_root(
    root: &Path,
    request: &RelayRequest,
    timeout_ms: u64,
) -> Result<TerminalControllerService> {
    TerminalControllerService::new(TerminalControllerOptions {
        root: root.to_path_buf(),
        user_id: request.owner_user_id.clone(),
        project_id: local_mcp_terminal_project_id(request),
        idle_timeout_ms: 1_000,
        max_wait_ms: timeout_ms,
        max_output_chars: MAX_TERMINAL_OUTPUT_BYTES,
        store: TerminalControllerStoreRef::new(Arc::new(LocalConnectorTerminalControllerStore)),
    })
    .map_err(|err| anyhow!(err))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::local_mcp_terminal_project_id;
    use crate::relay::RelayRequest;

    #[test]
    fn task_run_id_isolated_terminal_context_takes_priority() {
        let request = RelayRequest {
            _message_type: "local_runtime_chat".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: Some("user-1".to_string()),
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            method: None,
            path: None,
            headers: BTreeMap::from([
                ("x-task-runner-task-id".to_string(), "project-1".to_string()),
                ("x-task-runner-run-id".to_string(), "run-1".to_string()),
            ]),
            body: json!({}),
        };
        assert_eq!(
            local_mcp_terminal_project_id(&request).as_deref(),
            Some("run-1")
        );
    }
}
