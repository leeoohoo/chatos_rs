use super::ai_runtime::run_ai_task_with_system_messages;
use super::*;

pub(super) fn suggest_sub_agent_text_with_docs(
    ctx: &BoundContext,
    task: &str,
    requested_model: Option<&str>,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
) -> Result<String, String> {
    let (repo_root, system_messages) = load_recommender_docs_for_suggest()?;
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

    let request_text = format!(
        "根据下面的 task 选择最合适的 agent 和 skills。\n\
只返回纯文本，不要 JSON、Markdown、代码块。\n\
输出格式严格为 3 行：\n\
agent_id: <agent-id>\n\
skills: <comma-separated-skill-ids or empty>\n\
reason: <short reason>\n\n\
task:\n{}",
        task
    );

    let ai = run_ai_task_with_system_messages(
        ctx,
        system_messages,
        request_text.as_str(),
        requested_model,
        on_stream_chunk,
    )?;

    Ok(ai.response)
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
