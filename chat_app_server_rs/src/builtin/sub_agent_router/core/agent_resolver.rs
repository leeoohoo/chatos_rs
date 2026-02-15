use super::super::*;

pub(crate) fn resolve_agent_and_command(
    ctx: &BoundContext,
    task: &str,
    args: &Value,
) -> Result<ResolvedAgent, String> {
    let agent_id = optional_trimmed_string(args, "agent_id")
        .ok_or_else(|| "agent_id is required. Call suggest_sub_agent first.".to_string())?;
    let command_id = optional_trimmed_string(args, "command_id");
    let skills = parse_string_array(args.get("skills"));
    let requested_skills = skills.clone().unwrap_or_default();
    let query = optional_trimmed_string(args, "query");

    let mut guard = ctx
        .catalog
        .lock()
        .map_err(|_| "catalog lock poisoned".to_string())?;
    let _ = guard.reload();

    let agent = guard
        .resolve_agent_for_task(
            agent_id.as_str(),
            task,
            query.as_deref(),
            command_id.as_deref(),
            requested_skills.as_slice(),
        )
        .ok_or_else(|| format!("Sub-agent {} not found.", agent_id))?;
    let command = guard.resolve_command(&agent, command_id.as_deref());
    if let Some(requested_command) = command_id.as_deref() {
        if command.is_none() {
            return Err(format!(
                "Command {} not found for sub-agent {}.",
                requested_command, agent.id
            ));
        }
    }
    let used_skills = select_skills(&agent, skills, &guard);

    Ok(ResolvedAgent {
        agent,
        command,
        used_skills,
        reason: agent_id,
    })
}

pub(crate) fn resolve_command_cwd(
    workspace_root: &Path,
    command_cwd: Option<&str>,
) -> Option<String> {
    let cwd = command_cwd
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            let path = PathBuf::from(value.as_str());
            if path.is_absolute() {
                path
            } else {
                workspace_root.join(path)
            }
        })
        .unwrap_or_else(|| workspace_root.to_path_buf());

    Some(cwd.to_string_lossy().to_string())
}
