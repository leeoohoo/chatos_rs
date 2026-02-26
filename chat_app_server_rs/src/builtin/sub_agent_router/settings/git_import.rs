use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use crate::builtin::sub_agent_router::types::{AgentSpec, RegistryData};
use crate::builtin::sub_agent_router::utils::ensure_dir;

use super::state::ensure_state_files;
use super::types::{
    GitImportOptions, RECOMMENDER_AGENTS_DOC_FILE, RECOMMENDER_REFERENCE_DOCS_DIR,
    RECOMMENDER_SKILLS_DOC_FILE,
};

pub(super) fn import_agents_json(raw: &str) -> Result<Value, String> {
    let parsed_value = serde_json::from_str::<Value>(raw)
        .map_err(|err| format!("agents JSON 解析失败: {}", err))?;

    let registry = if parsed_value.is_array() {
        let agents: Vec<AgentSpec> = serde_json::from_value(parsed_value)
            .map_err(|err| format!("agents 数组结构不正确: {}", err))?;
        RegistryData { agents }
    } else {
        serde_json::from_value::<RegistryData>(parsed_value)
            .map_err(|err| format!("agents 对象结构不正确: {}", err))?
    };

    let paths = ensure_state_files()?;
    let text = serde_json::to_string_pretty(&registry).map_err(|err| err.to_string())?;
    fs::write(paths.registry_path.as_path(), text).map_err(|err| err.to_string())?;

    Ok(json!({
        "ok": true,
        "agents": registry.agents.len(),
        "path": paths.registry_path.to_string_lossy().to_string()
    }))
}

pub(super) fn import_marketplace_json(raw: &str) -> Result<Value, String> {
    let parsed_value = serde_json::from_str::<Value>(raw)
        .map_err(|err| format!("skills/marketplace JSON 解析失败: {}", err))?;

    if !parsed_value.is_object() {
        return Err("skills/marketplace 必须是 JSON 对象，且包含 plugins 数组".to_string());
    }

    let plugins = parsed_value
        .get("plugins")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "skills/marketplace JSON 缺少 plugins 数组".to_string())?;

    let paths = ensure_state_files()?;
    let text = serde_json::to_string_pretty(&parsed_value).map_err(|err| err.to_string())?;
    fs::write(paths.marketplace_path.as_path(), text).map_err(|err| err.to_string())?;

    Ok(json!({
        "ok": true,
        "plugins": plugins.len(),
        "path": paths.marketplace_path.to_string_lossy().to_string()
    }))
}

pub(super) fn import_from_git(opts: GitImportOptions) -> Result<Value, String> {
    let paths = ensure_state_files()?;

    let repository = opts.repository.trim().to_string();
    if repository.is_empty() {
        return Err("repository 不能为空".to_string());
    }

    let branch = opts
        .branch
        .as_deref()
        .map(str::trim)
        .map(ToString::to_string)
        .filter(|v| !v.is_empty());

    let git_cache_root = paths.root.join("git-cache");
    let repo_root = ensure_git_repo(repository.as_str(), branch.as_deref(), &git_cache_root)?;

    let agents_file = resolve_repo_file(
        repo_root.as_path(),
        opts.agents_path.as_deref(),
        &["subagents.json", "agents.json"],
    )?;
    let skills_file = resolve_repo_file(
        repo_root.as_path(),
        opts.skills_path.as_deref(),
        &["marketplace.json", "skills.json"],
    )?;

    if agents_file.is_none() && skills_file.is_none() {
        return Err(
            "仓库里没有找到可导入文件（agents: subagents.json/agents.json, skills: marketplace.json/skills.json）"
                .to_string(),
        );
    }

    let mut imported_agents = false;
    let mut imported_skills = false;
    let mut agents_result: Option<Value> = None;
    let mut skills_result: Option<Value> = None;
    let imported_reference_docs =
        copy_recommender_reference_docs_from_repo(repo_root.as_path(), paths.root.as_path())
            .unwrap_or_else(|err| {
                json!({
                    "copied": 0,
                    "skipped": 2,
                    "error": err,
                    "details": []
                })
            });
    let mut copied_plugins = json!({
        "copied": 0,
        "skipped": 0,
        "details": []
    });

    if let Some(path) = agents_file.as_ref() {
        let raw = fs::read_to_string(path)
            .map_err(|err| format!("读取 agents 文件失败 ({}): {}", path.to_string_lossy(), err))?;
        agents_result = Some(import_agents_json(raw.as_str())?);
        imported_agents = true;
    }

    if let Some(path) = skills_file.as_ref() {
        let raw = fs::read_to_string(path)
            .map_err(|err| format!("读取 skills 文件失败 ({}): {}", path.to_string_lossy(), err))?;
        copied_plugins = copy_plugin_sources_from_repo(
            repo_root.as_path(),
            paths.plugins_root.as_path(),
            raw.as_str(),
        )?;
        skills_result = Some(import_marketplace_json(raw.as_str())?);
        imported_skills = true;
    }

    Ok(json!({
        "ok": true,
        "repository": repository,
        "branch": branch,
        "repo_path": repo_root.to_string_lossy().to_string(),
        "imported": {
            "agents": imported_agents,
            "skills": imported_skills
        },
        "files": {
            "agents": agents_file.map(|p| p.to_string_lossy().to_string()),
            "skills": skills_file.map(|p| p.to_string_lossy().to_string())
        },
        "results": {
            "agents": agents_result,
            "skills": skills_result,
            "plugins": copied_plugins,
            "reference_docs": imported_reference_docs
        }
    }))
}

fn resolve_repo_file(
    repo_root: &Path,
    hint: Option<&str>,
    default_candidates: &[&str],
) -> Result<Option<PathBuf>, String> {
    if let Some(path) = hint
        .map(normalize_repo_relative_path)
        .filter(|v| !v.is_empty())
    {
        let resolved = repo_root.join(path.as_str());
        if !resolved.exists() {
            return Err(format!(
                "specified path does not exist: {}",
                resolved.to_string_lossy()
            ));
        }

        if resolved.is_file() {
            return Ok(Some(resolved));
        }

        if resolved.is_dir() {
            if let Some(candidate) =
                find_default_file_recursively(resolved.as_path(), default_candidates)
            {
                return Ok(Some(candidate));
            }
            return Err(format!(
                "no candidate file found in directory ({}) : {}",
                default_candidates.join(", "),
                resolved.to_string_lossy()
            ));
        }

        return Err(format!(
            "specified path is not a regular file: {}",
            resolved.to_string_lossy()
        ));
    }

    for rel in default_candidates {
        let candidate = repo_root.join(rel);
        if candidate.exists() && candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    Ok(find_default_file_recursively(repo_root, default_candidates))
}

fn normalize_repo_relative_path(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

fn find_default_file_recursively(root: &Path, default_candidates: &[&str]) -> Option<PathBuf> {
    let mut candidate_names = HashSet::new();
    for rel in default_candidates {
        let name = Path::new(rel).file_name().and_then(|v| v.to_str())?;
        candidate_names.insert(name.to_string());
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(dir.as_path()) {
            Ok(value) => value,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
                    continue;
                };
                if candidate_names.contains(name) {
                    return Some(path);
                }
                continue;
            }

            if path.is_dir() && !is_skipped_repo_dir(path.as_path()) {
                stack.push(path);
            }
        }
    }

    None
}

fn is_skipped_repo_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
        return false;
    };

    matches!(name, ".git" | "node_modules" | "target" | ".next")
}

fn ensure_git_repo(
    repo_url: &str,
    branch: Option<&str>,
    cache_root: &Path,
) -> Result<PathBuf, String> {
    ensure_dir(cache_root)?;
    let safe_name = sanitize_repo_name(repo_url);
    let repo_path = cache_root.join(safe_name);

    if repo_path.exists() {
        fs::remove_dir_all(repo_path.as_path())
            .map_err(|err| format!("清理旧仓库失败 ({}): {}", repo_path.to_string_lossy(), err))?;
    }

    let mut args = vec!["clone".to_string(), "--depth".to_string(), "1".to_string()];
    if let Some(ref_name) = branch {
        args.push("--branch".to_string());
        args.push(ref_name.to_string());
    }
    args.push(repo_url.to_string());
    args.push(repo_path.to_string_lossy().to_string());
    run_git(args.as_slice())?;

    Ok(repo_path)
}

fn run_git(args: &[String]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|err| format!("git 执行失败: {}", err))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!(
        "git 命令失败 (exit={}): {}",
        output.status.code().unwrap_or(-1),
        detail
    ))
}

fn sanitize_repo_name(value: &str) -> String {
    let mut raw = value.trim().to_string();
    if let Some(stripped) = raw.strip_prefix("https://") {
        raw = stripped.to_string();
    } else if let Some(stripped) = raw.strip_prefix("http://") {
        raw = stripped.to_string();
    }
    if let Some(stripped) = raw.strip_prefix("git@") {
        raw = stripped.to_string();
    }

    raw = raw.replace([':', '/'], "-");
    if raw.ends_with(".git") {
        raw.truncate(raw.len().saturating_sub(4));
    }

    let mut cleaned = String::new();
    let mut last_dash = false;
    for ch in raw.chars() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        if valid {
            cleaned.push(ch);
            last_dash = false;
        } else if !last_dash {
            cleaned.push('-');
            last_dash = true;
        }
    }

    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "repo".to_string()
    } else {
        trimmed
    }
}

fn recommender_reference_docs_root(state_root: &Path) -> PathBuf {
    state_root.join(RECOMMENDER_REFERENCE_DOCS_DIR)
}

fn copy_recommender_reference_docs_from_repo(
    repo_root: &Path,
    state_root: &Path,
) -> Result<Value, String> {
    let docs_root = recommender_reference_docs_root(state_root);
    ensure_dir(docs_root.as_path())?;

    let specs = [
        (
            RECOMMENDER_AGENTS_DOC_FILE,
            [
                "docs/agents.md",
                "docs/agents/docs/agents.md",
                "chat_app_server_rs/docs/agents/docs/agents.md",
            ]
            .as_slice(),
        ),
        (
            RECOMMENDER_SKILLS_DOC_FILE,
            [
                "docs/agent-skills.md",
                "docs/agents/docs/agent-skills.md",
                "chat_app_server_rs/docs/agents/docs/agent-skills.md",
            ]
            .as_slice(),
        ),
    ];

    let mut copied = 0usize;
    let mut skipped = 0usize;
    let mut details = Vec::new();

    for (file_name, preferred_paths) in specs {
        let source = find_repo_doc_for_recommender(repo_root, preferred_paths, file_name);
        let Some(source) = source else {
            skipped += 1;
            details.push(json!({
                "name": file_name,
                "ok": false,
                "reason": "not found in repository"
            }));
            continue;
        };

        let destination = docs_root.join(file_name);
        match copy_reference_doc_file(source.as_path(), destination.as_path()) {
            Ok(_) => {
                copied += 1;
                details.push(json!({
                    "name": file_name,
                    "ok": true,
                    "source": source.to_string_lossy().to_string(),
                    "dest": destination.to_string_lossy().to_string()
                }));
            }
            Err(err) => {
                skipped += 1;
                details.push(json!({
                    "name": file_name,
                    "ok": false,
                    "source": source.to_string_lossy().to_string(),
                    "reason": err
                }));
            }
        }
    }

    Ok(json!({
        "copied": copied,
        "skipped": skipped,
        "details": details
    }))
}

fn find_repo_doc_for_recommender(
    repo_root: &Path,
    preferred_paths: &[&str],
    file_name: &str,
) -> Option<PathBuf> {
    for rel in preferred_paths {
        let normalized = normalize_repo_relative_path(rel);
        if normalized.is_empty() {
            continue;
        }
        let candidate = repo_root.join(normalized.as_str());
        if candidate.exists() && candidate.is_file() {
            return Some(candidate);
        }
    }

    find_default_file_recursively(repo_root, &[file_name])
}

fn copy_reference_doc_file(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() || !src.is_file() {
        return Err(format!("source not found: {}", src.to_string_lossy()));
    }

    if let Some(parent) = dest.parent() {
        ensure_dir(parent)?;
    }

    fs::copy(src, dest).map_err(|err| err.to_string())?;
    Ok(())
}

fn copy_plugin_sources_from_repo(
    repo_root: &Path,
    plugins_root: &Path,
    marketplace_raw: &str,
) -> Result<Value, String> {
    let parsed = serde_json::from_str::<Value>(marketplace_raw)
        .map_err(|err| format!("skills/marketplace JSON 解析失败: {}", err))?;

    let plugins = parsed
        .get("plugins")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut copied = 0usize;
    let mut skipped = 0usize;
    let mut details = Vec::new();

    for plugin in plugins {
        let source_raw = plugin
            .get("source")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        let source = normalize_repo_relative_path(source_raw.as_str());
        if Path::new(source_raw.as_str()).is_absolute() {
            skipped += 1;
            details.push(json!({
                "source": source_raw,
                "ok": false,
                "reason": "absolute source is not allowed"
            }));
            continue;
        }

        if source.is_empty() {
            skipped += 1;
            details.push(json!({
                "source": source_raw,
                "ok": false,
                "reason": "missing source"
            }));
            continue;
        }

        if has_parent_path_component(source.as_str()) {
            skipped += 1;
            details.push(json!({
                "source": source,
                "ok": false,
                "reason": "source cannot contain .."
            }));
            continue;
        }

        let src = repo_root.join(source.as_str());
        if !src.exists() {
            skipped += 1;
            details.push(json!({
                "source": source,
                "ok": false,
                "reason": format!("source not found in repo: {}", src.to_string_lossy())
            }));
            continue;
        }

        let dest_rel = plugin_install_destination(source.as_str());
        if dest_rel.is_empty() {
            skipped += 1;
            details.push(json!({
                "source": source,
                "ok": false,
                "reason": "invalid source after normalization"
            }));
            continue;
        }

        let dest = plugins_root.join(dest_rel.as_str());
        match copy_path(src.as_path(), dest.as_path()) {
            Ok(_) => {
                copied += 1;
                details.push(json!({
                    "source": source,
                    "ok": true,
                    "dest": dest.to_string_lossy().to_string(),
                    "dest_relative": dest_rel
                }));
            }
            Err(err) => {
                skipped += 1;
                details.push(json!({
                    "source": source,
                    "ok": false,
                    "reason": err
                }));
            }
        }
    }

    Ok(json!({
        "copied": copied,
        "skipped": skipped,
        "details": details
    }))
}

fn plugin_install_destination(source: &str) -> String {
    let normalized = normalize_repo_relative_path(source);
    if let Some(stripped) = normalized.strip_prefix("plugins/") {
        stripped.trim_matches('/').to_string()
    } else {
        normalized
    }
}

fn has_parent_path_component(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn copy_path(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("source not found: {}", src.to_string_lossy()));
    }

    if dest.exists() {
        if dest.is_dir() {
            fs::remove_dir_all(dest).map_err(|err| err.to_string())?;
        } else {
            fs::remove_file(dest).map_err(|err| err.to_string())?;
        }
    }

    if src.is_file() {
        if let Some(parent) = dest.parent() {
            ensure_dir(parent)?;
        }
        fs::copy(src, dest).map_err(|err| err.to_string())?;
        return Ok(());
    }

    ensure_dir(dest)?;
    for entry in fs::read_dir(src).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let next = dest.join(entry.file_name());
        let file_type = entry.file_type().map_err(|err| err.to_string())?;
        if file_type.is_dir() {
            copy_path(path.as_path(), next.as_path())?;
        } else if file_type.is_file() {
            if let Some(parent) = next.parent() {
                ensure_dir(parent)?;
            }
            fs::copy(path.as_path(), next.as_path()).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}
