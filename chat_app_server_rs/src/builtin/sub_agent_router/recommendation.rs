use super::ai_runtime::run_ai_task_with_system_messages;
use super::*;

pub(super) fn suggest_sub_agent_text_with_docs(
    ctx: &BoundContext,
    task: &str,
    requested_model: Option<&str>,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
) -> Result<String, String> {
    let (repo_root, mut system_messages) = load_recommender_docs_for_suggest()?;
    trace_router_node(
        "suggest_sub_agent",
        "docs_loaded",
        None,
        None,
        None,
        Some(json!({
            "repo_root": repo_root.to_string_lossy().to_string(),
            "docs_count": system_messages.len(),
        })),
    );

    let live_catalog_text = build_live_agent_catalog_for_suggest(ctx)?;
    if !live_catalog_text.trim().is_empty() {
        trace_router_node(
            "suggest_sub_agent",
            "live_catalog_ready",
            None,
            None,
            None,
            Some(json!({
                "chars": live_catalog_text.chars().count(),
                "preview": truncate_for_event(live_catalog_text.as_str(), 2_000),
            })),
        );
        system_messages.push(live_catalog_text);
    }

    let request_text = format!(
        "Choose the best sub-agent and skills for the task.\nOutput plain text with exactly 3 lines (no JSON/Markdown/code block):\nagent_id: <agent-id>\nskills: <comma-separated-skill-ids or empty>\nreason: <short reason>\n\nRules:\n1) You MUST choose agent_id from the live catalog exactly (including folder prefix).\n2) Prefer the agent whose command/skill/profile best matches this task.\n\ntask:\n{}",
        task
    );

    let ai = run_ai_task_with_system_messages(
        ctx,
        system_messages,
        request_text.as_str(),
        requested_model,
        on_stream_chunk,
    )?;

    Ok(normalize_suggest_result(ctx, task, ai.response.as_str()))
}

fn build_live_agent_catalog_for_suggest(ctx: &BoundContext) -> Result<String, String> {
    let mut guard = ctx
        .catalog
        .lock()
        .map_err(|_| "catalog lock poisoned".to_string())?;
    let _ = guard.reload();

    let agents = guard.list_agents();
    if agents.is_empty() {
        return Ok(String::new());
    }

    let max_items = 400usize;
    let mut lines = Vec::new();
    lines.push(
        "Live agent catalog (must choose one exact agent_id from this list):".to_string(),
    );

    for agent in agents.iter().take(max_items) {
        let command_ids = agent
            .commands
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|command| command.id)
            .take(4)
            .collect::<Vec<_>>();
        let skills = agent
            .skills
            .clone()
            .unwrap_or_default()
            .into_iter()
            .take(4)
            .collect::<Vec<_>>();

        lines.push(format!(
            "- agent_id: {} | name: {} | plugin: {} | commands: {} | skills: {} | desc: {}",
            agent.id,
            normalize_field(agent.name.as_str(), 80),
            normalize_field(agent.plugin.as_deref().unwrap_or(""), 40),
            if command_ids.is_empty() {
                "(none)".to_string()
            } else {
                command_ids.join(",")
            },
            if skills.is_empty() {
                "(none)".to_string()
            } else {
                skills.join(",")
            },
            normalize_field(agent.description.as_deref().unwrap_or(""), 120),
        ));
    }

    if agents.len() > max_items {
        lines.push(format!(
            "- ... truncated {} more agents",
            agents.len() - max_items
        ));
    }

    Ok(lines.join("\n"))
}

fn normalize_field(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let normalized = value
        .replace('\r', " ")
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    normalized.chars().take(max_chars).collect::<String>() + "..."
}

fn normalize_suggest_result(ctx: &BoundContext, task: &str, ai_text: &str) -> String {
    let Some(requested_agent_id) = extract_keyed_line_value(ai_text, "agent_id") else {
        return ai_text.to_string();
    };

    let requested_skills = extract_skills_from_text(ai_text);

    let mut guard = match ctx.catalog.lock() {
        Ok(guard) => guard,
        Err(_) => return ai_text.to_string(),
    };
    let _ = guard.reload();

    let Some(resolved) = guard.resolve_agent_for_task(
        requested_agent_id.as_str(),
        task,
        None,
        None,
        requested_skills.as_slice(),
    ) else {
        return ai_text.to_string();
    };

    if resolved.id == requested_agent_id {
        return ai_text.to_string();
    }

    replace_keyed_line_value(ai_text, "agent_id", resolved.id.as_str())
}

fn extract_keyed_line_value(text: &str, key: &str) -> Option<String> {
    let key_prefix = format!("{}:", key.to_lowercase());

    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if !lower.starts_with(key_prefix.as_str()) {
            continue;
        }

        let value = trimmed
            .split_once(':')
            .map(|(_, rest)| rest.trim().to_string())
            .unwrap_or_default();
        if !value.is_empty() {
            return Some(value);
        }
    }

    None
}

fn extract_skills_from_text(text: &str) -> Vec<String> {
    extract_keyed_line_value(text, "skills")
        .map(|raw| {
            raw.split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn replace_keyed_line_value(text: &str, key: &str, new_value: &str) -> String {
    let key_lower = key.to_lowercase();
    let mut lines = Vec::new();
    let mut replaced = false;

    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if !replaced && lower.starts_with(format!("{}:", key_lower).as_str()) {
            lines.push(format!("{}: {}", key, new_value));
            replaced = true;
        } else {
            lines.push(line.to_string());
        }
    }

    if !replaced {
        lines.insert(0, format!("{}: {}", key, new_value));
    }

    lines.join("\n")
}

fn load_recommender_docs_for_suggest() -> Result<(PathBuf, Vec<String>), String> {
    let repo_root = resolve_latest_git_cache_repo()?;
    let agents_doc_path = repo_root.join("docs/agents.md");
    let skills_doc_path = repo_root.join("docs/agent-skills.md");

    let agents_doc = read_required_doc(agents_doc_path.as_path())?;
    let skills_doc = read_required_doc(skills_doc_path.as_path())?;

    Ok((repo_root, vec![agents_doc, skills_doc]))
}

fn resolve_latest_git_cache_repo() -> Result<PathBuf, String> {
    let paths = settings::ensure_state_files()
        .map_err(|err| format!("Failed to resolve state paths: {}", err))?;

    let git_cache_root = paths.root.join("git-cache");
    if !git_cache_root.exists() {
        return Err(format!(
            "Git cache directory not found: {}",
            git_cache_root.to_string_lossy()
        ));
    }

    let mut repo_dirs = std::fs::read_dir(git_cache_root.as_path())
        .map_err(|err| {
            format!(
                "Failed to read git cache directory {}: {}",
                git_cache_root.to_string_lossy(),
                err
            )
        })?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();

    if repo_dirs.is_empty() {
        return Err(format!(
            "No cached repositories found under: {}",
            git_cache_root.to_string_lossy()
        ));
    }

    repo_dirs.sort_by(|left, right| {
        let left_modified = modified_time_or_epoch(left.as_path());
        let right_modified = modified_time_or_epoch(right.as_path());
        right_modified.cmp(&left_modified)
    });

    repo_dirs
        .into_iter()
        .next()
        .ok_or_else(|| "No cached repositories found".to_string())
}

fn modified_time_or_epoch(path: &Path) -> std::time::SystemTime {
    path.metadata()
        .ok()
        .and_then(|meta| meta.modified().ok())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}

fn read_required_doc(path: &Path) -> Result<String, String> {
    if !path.exists() || !path.is_file() {
        return Err(format!(
            "Required doc not found: {}",
            path.to_string_lossy()
        ));
    }

    let content = std::fs::read_to_string(path)
        .map_err(|err| format!("Failed to read {}: {}", path.to_string_lossy(), err))?;
    let trimmed = content.trim();

    if trimmed.is_empty() {
        Err(format!("Required doc is empty: {}", path.to_string_lossy()))
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_keyed_line_updates_existing_key() {
        let input = "agent_id: code-reviewer\nskills: a,b\nreason: test";
        let updated = replace_keyed_line_value(input, "agent_id", "application-performance/code-reviewer");
        assert!(updated.contains("agent_id: application-performance/code-reviewer"));
        assert!(updated.contains("skills: a,b"));
    }

    #[test]
    fn extract_skills_parses_comma_values() {
        let input = "agent_id: x\nskills: a, b ,c\nreason: r";
        let skills = extract_skills_from_text(input);
        assert_eq!(skills, vec!["a", "b", "c"]);
    }
}
