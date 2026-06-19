use std::path::Path;

use serde_json::{Value, json};
use tokio::time::Duration;

use crate::models::terminal::{TERMINAL_KIND_SHARED, Terminal, TerminalService};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::services::terminal_manager::get_terminal_manager;

use super::super::BoundContext;
use super::super::capture::capture_command_output;
use super::super::context::{
    build_input_payload, derive_terminal_name, resolve_project_root, resolve_target_path,
    terminal_cwd_in_root,
};

pub(in crate::builtin::terminal_controller) async fn execute_command_with_context(
    ctx: BoundContext,
    path_input: &str,
    command: &str,
    background: bool,
) -> Result<Value, String> {
    let (project_id, project_root) = resolve_project_root(&ctx).await?;
    let target_path = resolve_target_path(project_root.as_path(), path_input)?;

    let manager = get_terminal_manager();
    let (terminal, reused) = if let Some(idle) =
        find_idle_terminal(&project_id, project_root.as_path(), ctx.user_id.as_deref()).await?
    {
        (idle, true)
    } else {
        let name = derive_terminal_name(project_root.as_path());
        let created = manager
            .create(
                name,
                project_root.to_string_lossy().to_string(),
                TERMINAL_KIND_SHARED.to_string(),
                ctx.user_id.clone(),
                project_id.clone(),
            )
            .await?;
        (created, false)
    };

    let session = manager.ensure_running(&terminal).await?;
    let mut receiver = session.subscribe();
    let terminal_id = terminal.id.clone();

    let input_data = build_input_payload(project_root.as_path(), target_path.as_path(), command);
    session.write_input(input_data.as_str())?;

    let trimmed_command = command.trim();
    if !trimmed_command.is_empty() {
        let cmd_log = TerminalLog::new(
            terminal.id.clone(),
            "command".to_string(),
            trimmed_command.to_string(),
        );
        let _ = TerminalLogService::create(cmd_log).await;
    }
    if !input_data.is_empty() {
        let input_log = TerminalLog::new(terminal.id.clone(), "input".to_string(), input_data);
        let _ = TerminalLogService::create(input_log).await;
    }
    let _ = TerminalService::touch(terminal.id.as_str()).await;

    if background {
        let busy = manager.get_busy(terminal.id.as_str()).unwrap_or(false);
        return Ok(json!({
            "project_id": project_id,
            "project_root": project_root.to_string_lossy(),
            "terminal_id": terminal_id.clone(),
            "process_id": terminal_id.clone(),
            "terminal_reused": reused,
            "path": target_path.to_string_lossy(),
            "common": command,
            "background": true,
            "busy": busy,
            "output": "",
            "output_chars": 0,
            "truncated": false,
            "finished_by": "background",
            "idle_timeout_ms": ctx.idle_timeout_ms,
            "max_wait_ms": ctx.max_wait_ms,
            "max_output_chars": ctx.max_output_chars
        }));
    }

    let capture = capture_command_output(
        &mut receiver,
        Duration::from_millis(ctx.idle_timeout_ms),
        Duration::from_millis(ctx.max_wait_ms),
        ctx.max_output_chars,
    )
    .await;

    Ok(json!({
        "project_id": project_id,
        "project_root": project_root.to_string_lossy(),
        "terminal_id": terminal_id.clone(),
        "process_id": terminal_id.clone(),
        "terminal_reused": reused,
        "path": target_path.to_string_lossy(),
        "common": command,
        "background": false,
        "busy": manager.get_busy(terminal_id.as_str()).unwrap_or(false),
        "output": capture.output,
        "output_chars": capture.output.chars().count(),
        "truncated": capture.truncated,
        "finished_by": capture.finished_by,
        "idle_timeout_ms": ctx.idle_timeout_ms,
        "max_wait_ms": ctx.max_wait_ms,
        "max_output_chars": ctx.max_output_chars
    }))
}

async fn find_idle_terminal(
    project_id: &Option<String>,
    project_root: &Path,
    user_id: Option<&str>,
) -> Result<Option<Terminal>, String> {
    let terminals = TerminalService::list(user_id.map(|value| value.to_string())).await?;
    let manager = get_terminal_manager();

    for terminal in terminals {
        if terminal.status == "exited" {
            continue;
        }

        if let Some(pid) = project_id.as_deref() {
            if terminal.project_id.as_deref() != Some(pid) {
                continue;
            }
        } else if !terminal_cwd_in_root(terminal.cwd.as_str(), project_root) {
            continue;
        }

        let busy = manager.get_busy(terminal.id.as_str()).unwrap_or(false);
        if !busy {
            return Ok(Some(terminal));
        }
    }

    Ok(None)
}
