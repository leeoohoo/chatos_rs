mod ai_mode;
mod command_mode;
mod stream_callbacks;

use std::sync::atomic::AtomicBool;

use super::super::*;

pub(crate) fn execute_job(
    execution: JobExecutionContext,
    cancel_flag: Option<&AtomicBool>,
) -> Result<(String, Value), String> {
    if let Some(flag) = cancel_flag {
        if flag.load(Ordering::Relaxed) {
            append_job_event(
                execution.job_id.as_str(),
                "execute_cancelled_precheck",
                Some(json!({ "reason": "cancel_flag" })),
                execution.session_id.as_str(),
                execution.run_id.as_str(),
            );
            return Ok((
                "cancelled".to_string(),
                json!({
                    "status": "cancelled",
                    "job_id": execution.job_id,
                    "agent_id": execution.resolved.agent.id,
                    "agent_name": execution.resolved.agent.name,
                    "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
                    "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                    "reason": execution.resolved.reason,
                    "error": "cancelled"
                }),
            ));
        }
    }

    let requested_model = optional_trimmed_string(&execution.args, "caller_model")
        .or_else(|| optional_trimmed_string(&execution.args, "model"))
        .map(|value| value.to_string());
    let allow_policy = resolve_allow_prefixes(execution.args.get("mcp_allow_prefixes"));
    append_job_event(
        execution.job_id.as_str(),
        "execute_prepare",
        Some(json!({
            "agent_id": execution.resolved.agent.id,
            "agent_name": execution.resolved.agent.name,
            "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
            "skills": execution
                .resolved
                .used_skills
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>(),
            "requested_model": requested_model.clone(),
            "allow_prefixes": allow_policy.prefixes.clone(),
            "query": optional_trimmed_string(&execution.args, "query").unwrap_or_default(),
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    let run_env = build_env(
        execution.task.as_str(),
        &execution.resolved.agent,
        execution.resolved.command.as_ref(),
        &execution.resolved.used_skills,
        execution.session_id.as_str(),
        execution.run_id.as_str(),
        optional_trimmed_string(&execution.args, "query").as_deref(),
        optional_trimmed_string(&execution.args, "model").as_deref(),
        optional_trimmed_string(&execution.args, "caller_model").as_deref(),
        &allow_policy.prefixes,
        execution.ctx.project_id.as_deref(),
    );
    append_job_event(
        execution.job_id.as_str(),
        "env_ready",
        Some(json!({
            "entries": run_env.len(),
            "keys": run_env.keys().cloned().collect::<Vec<_>>(),
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    if let Some(cmd) = execution
        .resolved
        .command
        .clone()
        .and_then(|command| command.exec)
    {
        append_job_event(
            execution.job_id.as_str(),
            "execute_mode_selected",
            Some(json!({
                "mode": "command",
            })),
            execution.session_id.as_str(),
            execution.run_id.as_str(),
        );

        return command_mode::execute_command_mode(&execution, cmd, &run_env, cancel_flag);
    }

    append_job_event(
        execution.job_id.as_str(),
        "execute_mode_selected",
        Some(json!({
            "mode": "ai",
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    ai_mode::execute_ai_mode(&execution, requested_model, &allow_policy)
}
