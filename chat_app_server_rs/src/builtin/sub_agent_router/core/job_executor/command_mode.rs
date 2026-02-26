use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use super::super::super::*;
use super::super::agent_resolver::resolve_command_cwd;

pub(super) fn execute_command_mode(
    execution: &JobExecutionContext,
    cmd: Vec<String>,
    run_env: &HashMap<String, String>,
    cancel_flag: Option<&AtomicBool>,
) -> Result<(String, Value), String> {
    let cwd = resolve_command_cwd(
        execution.ctx.workspace_root.as_path(),
        execution
            .resolved
            .command
            .as_ref()
            .and_then(|command| command.cwd.as_deref()),
    );

    append_job_event(
        execution.job_id.as_str(),
        "command_start",
        Some(json!({
            "command": cmd.clone(),
            "cwd": cwd,
            "timeout_ms": execution.ctx.timeout_ms,
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    let result = run_command(
        &cmd,
        run_env,
        cwd.as_deref(),
        execution.ctx.timeout_ms,
        execution.ctx.max_output_bytes,
        None,
        cancel_flag,
    )?;

    let status = if matches!(result.error.as_deref(), Some("cancelled")) {
        "cancelled".to_string()
    } else if result.exit_code.unwrap_or(0) == 0 && !result.timed_out {
        "ok".to_string()
    } else {
        "error".to_string()
    };

    let payload = json!({
        "status": status,
        "job_id": execution.job_id,
        "agent_id": execution.resolved.agent.id,
        "agent_name": execution.resolved.agent.name,
        "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
        "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        "reason": execution.resolved.reason,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "exit_code": result.exit_code,
        "signal": result.signal,
        "duration_ms": result.duration_ms,
        "started_at": result.started_at,
        "finished_at": result.finished_at,
        "stdout_truncated": result.stdout_truncated,
        "stderr_truncated": result.stderr_truncated,
        "error": result.error,
        "timed_out": result.timed_out,
    });

    append_job_event(
        execution.job_id.as_str(),
        "command_finish",
        Some(json!({
            "status": payload.get("status").cloned().unwrap_or(Value::String("error".to_string())),
            "exit_code": result.exit_code,
            "signal": result.signal,
            "duration_ms": result.duration_ms,
            "timed_out": result.timed_out,
            "error": result.error,
            "stdout_preview": truncate_for_event(result.stdout.as_str(), 2000),
            "stderr_preview": truncate_for_event(result.stderr.as_str(), 2000),
            "stdout_truncated": result.stdout_truncated,
            "stderr_truncated": result.stderr_truncated,
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    Ok((
        payload
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("error")
            .to_string(),
        payload,
    ))
}
