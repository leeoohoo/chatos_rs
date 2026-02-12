use super::super::*;

pub(crate) fn resolve_agent_and_command(
    ctx: &BoundContext,
    task: &str,
    args: &Value,
) -> Result<ResolvedAgent, String> {
    let agent_id = optional_trimmed_string(args, "agent_id");
    let command_id = optional_trimmed_string(args, "command_id");
    let category = optional_trimmed_string(args, "category");
    let query = optional_trimmed_string(args, "query");
    let skills = parse_string_array(args.get("skills"));
    let caller_model = optional_trimmed_string(args, "caller_model")
        .or_else(|| optional_trimmed_string(args, "model"));

    let mut guard = ctx
        .catalog
        .lock()
        .map_err(|_| "catalog lock poisoned".to_string())?;
    let _ = guard.reload();

    if let Some(id) = agent_id {
        let agent = guard
            .get_agent(id.as_str())
            .ok_or_else(|| format!("Sub-agent {} not found.", id))?;
        let command = guard.resolve_command(&agent, command_id.as_deref());
        let used_skills = select_skills(&agent, skills, &guard);
        return Ok(ResolvedAgent {
            agent,
            command,
            used_skills,
            reason: id,
        });
    }

    let agents = guard.list_agents();
    if agents.is_empty() {
        return Err("No sub-agents available. Import agents/skills first.".to_string());
    }

    let candidates = build_agent_recommendation_candidates(&agents, &guard);
    drop(guard);

    let picked = pick_agent_with_llm(
        ctx,
        &agents,
        &candidates,
        task,
        category.clone(),
        skills.clone(),
        query.clone(),
        command_id.clone(),
        caller_model.as_deref(),
    )
    .or_else(|| {
        pick_agent_with_fallback(&agents, task, category, skills, query, command_id.clone())
    })
    .or_else(|| pick_first_available_agent(&agents))
    .ok_or_else(|| "No sub-agents available. Import agents/skills first.".to_string())?;

    let mut guard = ctx
        .catalog
        .lock()
        .map_err(|_| "catalog lock poisoned".to_string())?;
    let _ = guard.reload();

    let agent = guard
        .get_agent(picked.agent.id.as_str())
        .unwrap_or_else(|| picked.agent.clone());

    let command = guard.resolve_command(&agent, command_id.as_deref());
    let used_skills = select_skills(&agent, Some(picked.used_skills.clone()), &guard);

    Ok(ResolvedAgent {
        agent,
        command,
        used_skills,
        reason: picked.reason,
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
