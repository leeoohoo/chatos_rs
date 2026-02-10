use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use crate::builtin::sub_agent_router::types::{AgentSpec, RegistryData};
use crate::builtin::sub_agent_router::utils::ensure_dir;

const SUB_AGENT_ROUTER_STATE_ROOT_ENV: &str = "SUB_AGENT_ROUTER_STATE_ROOT";

#[derive(Debug, Clone)]
pub struct SubAgentRouterStatePaths {
    pub root: PathBuf,
    pub registry_path: PathBuf,
    pub marketplace_path: PathBuf,
    pub plugins_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GitImportOptions {
    pub repository: String,
    pub branch: Option<String>,
    pub agents_path: Option<String>,
    pub skills_path: Option<String>,
}

pub fn resolve_state_paths() -> SubAgentRouterStatePaths {
    let root = resolve_state_root();
    SubAgentRouterStatePaths {
        registry_path: root.join("subagents.json"),
        marketplace_path: root.join("marketplace.json"),
        plugins_root: root.join("plugins"),
        root,
    }
}

pub fn ensure_state_files() -> Result<SubAgentRouterStatePaths, String> {
    let paths = resolve_state_paths();
    ensure_dir(paths.root.as_path())?;
    ensure_dir(paths.plugins_root.as_path())?;

    if !paths.registry_path.exists() {
        let initial = RegistryData { agents: Vec::new() };
        let text = serde_json::to_string_pretty(&initial).map_err(|err| err.to_string())?;
        fs::write(paths.registry_path.as_path(), text).map_err(|err| err.to_string())?;
    }

    if !paths.marketplace_path.exists() {
        let initial = json!({
            "name": "sub-agent-router-marketplace",
            "plugins": []
        });
        let text = serde_json::to_string_pretty(&initial).map_err(|err| err.to_string())?;
        fs::write(paths.marketplace_path.as_path(), text).map_err(|err| err.to_string())?;
    }

    Ok(paths)
}

pub fn load_settings_summary() -> Result<Value, String> {
    let paths = ensure_state_files()?;
    let registry_raw = fs::read_to_string(paths.registry_path.as_path()).unwrap_or_default();
    let marketplace_raw = fs::read_to_string(paths.marketplace_path.as_path()).unwrap_or_default();

    let (registry_agents, registry_ok) = parse_registry_agents(registry_raw.as_str());
    let (plugins, marketplace_agents, skills, marketplace_ok) =
        parse_marketplace_items(marketplace_raw.as_str());

    let mut agent_items = registry_agents
        .iter()
        .map(|agent| {
            json!({
                "kind": "registry",
                "id": agent.id,
                "name": agent.name,
                "category": agent.category.clone().unwrap_or_default(),
                "skills": agent.skills.clone().unwrap_or_default()
            })
        })
        .collect::<Vec<_>>();

    let marketplace_agent_count = marketplace_agents.len();
    let skills_count = skills.len();
    agent_items.extend(marketplace_agents);

    Ok(json!({
        "paths": {
            "root": paths.root.to_string_lossy().to_string(),
            "registry": paths.registry_path.to_string_lossy().to_string(),
            "marketplace": paths.marketplace_path.to_string_lossy().to_string(),
            "plugins_root": paths.plugins_root.to_string_lossy().to_string(),
            "git_cache_root": paths.root.join("git-cache").to_string_lossy().to_string()
        },
        "counts": {
            "agents": agent_items.len(),
            "registry_agents": registry_agents.len(),
            "marketplace_agents": marketplace_agent_count,
            "plugins": plugins.len(),
            "skills_entries": skills_count
        },
        "valid": {
            "registry": registry_ok,
            "marketplace": marketplace_ok
        },
        "items": {
            "agents": agent_items,
            "skills": skills,
            "plugins": plugins
        }
    }))
}

pub fn import_agents_json(raw: &str) -> Result<Value, String> {
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

pub fn import_marketplace_json(raw: &str) -> Result<Value, String> {
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

pub fn import_from_git(opts: GitImportOptions) -> Result<Value, String> {
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
            "plugins": copied_plugins
        }
    }))
}

fn resolve_state_root() -> PathBuf {
    if let Ok(raw) = std::env::var(SUB_AGENT_ROUTER_STATE_ROOT_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".chatos").join("builtin_sub_agent_router")
}

fn parse_registry_agents(raw: &str) -> (Vec<AgentSpec>, bool) {
    if raw.trim().is_empty() {
        return (Vec::new(), false);
    }
    if let Ok(registry) = serde_json::from_str::<RegistryData>(raw) {
        return (registry.agents, true);
    }
    if let Ok(agents) = serde_json::from_str::<Vec<AgentSpec>>(raw) {
        return (agents, true);
    }
    (Vec::new(), false)
}

fn parse_marketplace_items(raw: &str) -> (Vec<Value>, Vec<Value>, Vec<Value>, bool) {
    if raw.trim().is_empty() {
        return (Vec::new(), Vec::new(), Vec::new(), false);
    }

    let parsed = match serde_json::from_str::<Value>(raw) {
        Ok(value) => value,
        Err(_) => return (Vec::new(), Vec::new(), Vec::new(), false),
    };

    let plugins_raw = parsed
        .get("plugins")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut plugins = Vec::new();
    let mut marketplace_agents = Vec::new();
    let mut skills = Vec::new();

    let mut seen_agent_keys = HashSet::new();
    let mut seen_skill_keys = HashSet::new();

    for plugin in plugins_raw {
        let source = plugin
            .get("source")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_default();

        let name = plugin
            .get("name")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                if source.is_empty() {
                    "plugin".to_string()
                } else {
                    source.clone()
                }
            });

        let agents_arr = plugin
            .get("agents")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let skills_arr = plugin
            .get("skills")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let commands_arr = plugin
            .get("commands")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        plugins.push(json!({
            "name": name,
            "source": source,
            "agents": agents_arr.len(),
            "skills": skills_arr.len(),
            "commands": commands_arr.len()
        }));

        for entry in agents_arr {
            let Some(path) = entry.as_str().map(|v| v.trim()).filter(|v| !v.is_empty()) else {
                continue;
            };
            let key = format!("{}::{}", source, path);
            if !seen_agent_keys.insert(key) {
                continue;
            }
            marketplace_agents.push(json!({
                "kind": "marketplace",
                "id": format!("{}:{}", source, path),
                "name": path,
                "plugin": name,
                "source": source,
                "path": path
            }));
        }

        for entry in skills_arr {
            let Some(path) = entry.as_str().map(|v| v.trim()).filter(|v| !v.is_empty()) else {
                continue;
            };
            let key = format!("{}::{}", source, path);
            if !seen_skill_keys.insert(key) {
                continue;
            }
            skills.push(json!({
                "id": format!("{}:{}", source, path),
                "name": path,
                "plugin": name,
                "source": source,
                "path": path
            }));
        }
    }

    (plugins, marketplace_agents, skills, true)
}

fn resolve_repo_file(
    repo_root: &Path,
    hint: Option<&str>,
    default_candidates: &[&str],
) -> Result<Option<PathBuf>, String> {
    if let Some(path) = hint.map(|v| v.trim()).filter(|v| !v.is_empty()) {
        let resolved = repo_root.join(path);
        if !resolved.exists() {
            return Err(format!("指定路径不存在: {}", resolved.to_string_lossy()));
        }
        if resolved.is_dir() {
            return Err(format!("指定路径不是文件: {}", resolved.to_string_lossy()));
        }
        return Ok(Some(resolved));
    }

    for rel in default_candidates {
        let candidate = repo_root.join(rel);
        if candidate.exists() && candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    Ok(None)
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
        let source = plugin
            .get("source")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or("")
            .to_string();

        if source.is_empty() {
            skipped += 1;
            details.push(json!({
                "source": source,
                "ok": false,
                "reason": "missing source"
            }));
            continue;
        }

        if Path::new(source.as_str()).is_absolute() {
            skipped += 1;
            details.push(json!({
                "source": source,
                "ok": false,
                "reason": "absolute source is not allowed"
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

        let dest = plugins_root.join(source.as_str());
        match copy_path(src.as_path(), dest.as_path()) {
            Ok(_) => {
                copied += 1;
                details.push(json!({
                    "source": source,
                    "ok": true,
                    "dest": dest.to_string_lossy().to_string()
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
