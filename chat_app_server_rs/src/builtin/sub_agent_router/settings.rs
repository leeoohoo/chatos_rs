use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::builtin::sub_agent_router::types::{AgentSpec, RegistryData};
use crate::builtin::sub_agent_router::utils::ensure_dir;

const SUB_AGENT_ROUTER_STATE_ROOT_ENV: &str = "SUB_AGENT_ROUTER_STATE_ROOT";
const RECOMMENDER_REFERENCE_DOCS_DIR: &str = "reference_docs";
const RECOMMENDER_AGENTS_DOC_FILE: &str = "agents.md";
const RECOMMENDER_SKILLS_DOC_FILE: &str = "agent-skills.md";

#[derive(Debug, Clone)]
pub struct SubAgentRouterStatePaths {
    pub root: PathBuf,
    pub registry_path: PathBuf,
    pub marketplace_path: PathBuf,
    pub plugins_root: PathBuf,
    pub mcp_permissions_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GitImportOptions {
    pub repository: String,
    pub branch: Option<String>,
    pub agents_path: Option<String>,
    pub skills_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstallPluginOptions {
    pub source: Option<String>,
    pub install_all: bool,
}

#[derive(Debug, Default, Clone)]
struct DiscoveredPluginEntries {
    exists: bool,
    agents: Vec<String>,
    skills: Vec<String>,
    commands: Vec<String>,
}

#[derive(Debug, Default)]
struct ParsedMarketplaceItems {
    plugins: Vec<Value>,
    agents: Vec<Value>,
    skills: Vec<Value>,
    discovered_agents: usize,
    discovered_skills: usize,
    discovered_commands: usize,
    installable_plugins: usize,
    valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SubAgentRouterMcpPermissions {
    configured: bool,
    enabled_mcp_ids: Vec<String>,
    enabled_tool_prefixes: Vec<String>,
    updated_at: String,
}

impl Default for SubAgentRouterMcpPermissions {
    fn default() -> Self {
        Self {
            configured: false,
            enabled_mcp_ids: Vec::new(),
            enabled_tool_prefixes: Vec::new(),
            updated_at: String::new(),
        }
    }
}

pub fn resolve_state_paths() -> SubAgentRouterStatePaths {
    let root = resolve_state_root();
    SubAgentRouterStatePaths {
        registry_path: root.join("subagents.json"),
        marketplace_path: root.join("marketplace.json"),
        plugins_root: root.join("plugins"),
        mcp_permissions_path: root.join("mcp_permissions.json"),
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

    if !paths.mcp_permissions_path.exists() {
        let text = serde_json::to_string_pretty(&SubAgentRouterMcpPermissions::default())
            .map_err(|err| err.to_string())?;
        fs::write(paths.mcp_permissions_path.as_path(), text).map_err(|err| err.to_string())?;
    }

    Ok(paths)
}

pub fn load_settings_summary() -> Result<Value, String> {
    let paths = ensure_state_files()?;
    let registry_raw = fs::read_to_string(paths.registry_path.as_path()).unwrap_or_default();
    let marketplace_raw = fs::read_to_string(paths.marketplace_path.as_path()).unwrap_or_default();

    let (registry_agents, registry_ok) = parse_registry_agents(registry_raw.as_str());
    let parsed_marketplace =
        parse_marketplace_items(marketplace_raw.as_str(), paths.plugins_root.as_path());

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

    let marketplace_agent_count = parsed_marketplace.agents.len();
    let skills_count = parsed_marketplace.skills.len();
    agent_items.extend(parsed_marketplace.agents.clone());

    Ok(json!({
        "paths": {
            "root": paths.root.to_string_lossy().to_string(),
            "registry": paths.registry_path.to_string_lossy().to_string(),
            "marketplace": paths.marketplace_path.to_string_lossy().to_string(),
            "plugins_root": paths.plugins_root.to_string_lossy().to_string(),
            "git_cache_root": paths.root.join("git-cache").to_string_lossy().to_string(),
            "mcp_permissions": paths.mcp_permissions_path.to_string_lossy().to_string()
        },
        "counts": {
            "agents": agent_items.len(),
            "registry_agents": registry_agents.len(),
            "marketplace_agents": marketplace_agent_count,
            "plugins": parsed_marketplace.plugins.len(),
            "skills_entries": skills_count,
            "discovered_agents": parsed_marketplace.discovered_agents,
            "discovered_skills": parsed_marketplace.discovered_skills,
            "discovered_commands": parsed_marketplace.discovered_commands,
            "installable_plugins": parsed_marketplace.installable_plugins
        },
        "valid": {
            "registry": registry_ok,
            "marketplace": parsed_marketplace.valid
        },
        "items": {
            "agents": agent_items,
            "skills": parsed_marketplace.skills,
            "plugins": parsed_marketplace.plugins
        }
    }))
}

pub fn load_mcp_permissions() -> Result<Value, String> {
    let paths = ensure_state_files()?;
    let state = read_mcp_permissions_state(paths.mcp_permissions_path.as_path())?;
    Ok(json!({
        "configured": state.configured,
        "enabled_mcp_ids": state.enabled_mcp_ids,
        "enabled_tool_prefixes": state.enabled_tool_prefixes,
        "updated_at": state.updated_at,
        "path": paths.mcp_permissions_path.to_string_lossy().to_string()
    }))
}

pub fn save_mcp_permissions(
    enabled_mcp_ids: &[String],
    enabled_tool_prefixes: &[String],
) -> Result<Value, String> {
    let paths = ensure_state_files()?;

    let mut ids = enabled_mcp_ids
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();

    let mut prefixes = enabled_tool_prefixes
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    prefixes.sort();
    prefixes.dedup();

    let state = SubAgentRouterMcpPermissions {
        configured: true,
        enabled_mcp_ids: ids,
        enabled_tool_prefixes: prefixes,
        updated_at: crate::core::time::now_rfc3339(),
    };

    let text = serde_json::to_string_pretty(&state).map_err(|err| err.to_string())?;
    fs::write(paths.mcp_permissions_path.as_path(), text).map_err(|err| err.to_string())?;

    Ok(json!({
        "ok": true,
        "configured": state.configured,
        "enabled_mcp_ids": state.enabled_mcp_ids,
        "enabled_tool_prefixes": state.enabled_tool_prefixes,
        "updated_at": state.updated_at,
        "path": paths.mcp_permissions_path.to_string_lossy().to_string()
    }))
}

fn read_mcp_permissions_state(path: &Path) -> Result<SubAgentRouterMcpPermissions, String> {
    let raw = fs::read_to_string(path).unwrap_or_default();
    if raw.trim().is_empty() {
        return Ok(SubAgentRouterMcpPermissions::default());
    }

    serde_json::from_str::<SubAgentRouterMcpPermissions>(raw.as_str()).or_else(|_| {
        let value = serde_json::from_str::<Value>(raw.as_str()).map_err(|err| err.to_string())?;
        let configured = value
            .get("configured")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut enabled_mcp_ids = value
            .get("enabled_mcp_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| entry.as_str())
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        enabled_mcp_ids.sort();
        enabled_mcp_ids.dedup();

        let mut enabled_tool_prefixes = value
            .get("enabled_tool_prefixes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| entry.as_str())
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        enabled_tool_prefixes.sort();
        enabled_tool_prefixes.dedup();

        let updated_at = value
            .get("updated_at")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .unwrap_or_default();

        Ok(SubAgentRouterMcpPermissions {
            configured,
            enabled_mcp_ids,
            enabled_tool_prefixes,
            updated_at,
        })
    })
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

pub fn load_recommender_reference_docs() -> Vec<(String, String)> {
    let paths = match ensure_state_files() {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let docs_root = recommender_reference_docs_root(paths.root.as_path());
    let mut docs = Vec::new();

    for file_name in [RECOMMENDER_AGENTS_DOC_FILE, RECOMMENDER_SKILLS_DOC_FILE] {
        let path = docs_root.join(file_name);
        let raw = fs::read_to_string(path.as_path()).unwrap_or_default();
        let content = raw.trim();
        if content.is_empty() {
            continue;
        }
        docs.push((file_name.to_string(), content.to_string()));
    }

    docs
}

pub fn install_plugins(opts: InstallPluginOptions) -> Result<Value, String> {
    let paths = ensure_state_files()?;
    let raw = fs::read_to_string(paths.marketplace_path.as_path())
        .map_err(|err| format!("读取 skills/marketplace 文件失败: {}", err))?;

    let mut parsed = serde_json::from_str::<Value>(raw.as_str())
        .map_err(|err| format!("skills/marketplace JSON 解析失败: {}", err))?;

    let plugins = parsed
        .get_mut("plugins")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| "skills/marketplace JSON 缺少 plugins 数组".to_string())?;

    let target = opts
        .source
        .as_deref()
        .map(normalize_plugin_source)
        .filter(|v| !v.is_empty());
    let target_key = target
        .as_ref()
        .map(|value| plugin_match_key(value.as_str()))
        .filter(|value| !value.is_empty());

    if !opts.install_all && target_key.is_none() {
        return Err("source 不能为空（或设置 install_all=true）".to_string());
    }

    let mut touched = 0usize;
    let mut installed = 0usize;
    let mut skipped = 0usize;
    let mut details = Vec::new();

    for plugin in plugins.iter_mut() {
        let source = plugin
            .get("source")
            .and_then(|v| v.as_str())
            .map(normalize_plugin_source)
            .unwrap_or_default();
        let name = plugin
            .get("name")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| source.clone());

        let source_key = plugin_match_key(source.as_str());
        let source_leaf_key = plugin_match_key(plugin_source_leaf(source.as_str()).as_str());
        let name_key = plugin_match_key(name.as_str());

        if source_key.is_empty() && name_key.is_empty() {
            continue;
        }

        let should_install = if opts.install_all {
            true
        } else {
            target_key
                .as_ref()
                .map(|target| {
                    (!source_key.is_empty() && target == &source_key)
                        || (!source_leaf_key.is_empty() && target == &source_leaf_key)
                        || (!name_key.is_empty() && target == &name_key)
                })
                .unwrap_or(false)
        };

        if !should_install {
            continue;
        }

        touched += 1;

        let discovered = discover_plugin_entries(paths.plugins_root.as_path(), source.as_str());
        let had_entries = plugin
            .as_object()
            .map(|obj| {
                has_non_empty_string_array(obj.get("agents"))
                    || has_non_empty_string_array(obj.get("skills"))
                    || has_non_empty_string_array(obj.get("commands"))
            })
            .unwrap_or(false);

        if !discovered.exists {
            skipped += 1;
            details.push(json!({
                "name": name,
                "source": source,
                "ok": false,
                "installed": false,
                "reason": "plugin source not found"
            }));
            continue;
        }

        if discovered.agents.is_empty()
            && discovered.skills.is_empty()
            && discovered.commands.is_empty()
        {
            skipped += 1;
            details.push(json!({
                "name": name,
                "source": source,
                "ok": false,
                "installed": false,
                "reason": "plugin source has no discoverable agents/skills/commands"
            }));
            continue;
        }

        if let Some(map) = plugin.as_object_mut() {
            map.insert(
                "agents".to_string(),
                Value::Array(
                    discovered
                        .agents
                        .iter()
                        .map(|item| Value::String(item.clone()))
                        .collect(),
                ),
            );
            map.insert(
                "skills".to_string(),
                Value::Array(
                    discovered
                        .skills
                        .iter()
                        .map(|item| Value::String(item.clone()))
                        .collect(),
                ),
            );
            map.insert(
                "commands".to_string(),
                Value::Array(
                    discovered
                        .commands
                        .iter()
                        .map(|item| Value::String(item.clone()))
                        .collect(),
                ),
            );
        }

        installed += 1;
        details.push(json!({
            "name": name,
            "source": source,
            "ok": true,
            "installed": true,
            "previously_installed": had_entries,
            "counts": {
                "agents": discovered.agents.len(),
                "skills": discovered.skills.len(),
                "commands": discovered.commands.len()
            }
        }));
    }

    if touched == 0 {
        return Err("未找到匹配的 plugin（请确认 source 或 name）".to_string());
    }

    let text = serde_json::to_string_pretty(&parsed).map_err(|err| err.to_string())?;
    fs::write(paths.marketplace_path.as_path(), text).map_err(|err| err.to_string())?;

    Ok(json!({
        "ok": true,
        "path": paths.marketplace_path.to_string_lossy().to_string(),
        "touched": touched,
        "installed": installed,
        "skipped": skipped,
        "details": details
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

fn parse_marketplace_items(raw: &str, plugins_root: &Path) -> ParsedMarketplaceItems {
    if raw.trim().is_empty() {
        return ParsedMarketplaceItems::default();
    }

    let parsed = match serde_json::from_str::<Value>(raw) {
        Ok(value) => value,
        Err(_) => return ParsedMarketplaceItems::default(),
    };

    let plugins_raw = parsed
        .get("plugins")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut output = ParsedMarketplaceItems {
        valid: true,
        ..ParsedMarketplaceItems::default()
    };
    let mut seen_agent_keys = HashSet::new();
    let mut seen_skill_keys = HashSet::new();

    for plugin in plugins_raw {
        let source = plugin
            .get("source")
            .and_then(|v| v.as_str())
            .map(normalize_plugin_source)
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

        let category = plugin
            .get("category")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_default();
        let description = plugin
            .get("description")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_default();
        let version = plugin
            .get("version")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_default();

        let agents_arr = value_to_string_list(plugin.get("agents"));
        let skills_arr = value_to_string_list(plugin.get("skills"));
        let commands_arr = value_to_string_list(plugin.get("commands"));

        let discovered = discover_plugin_entries(plugins_root, source.as_str());
        output.discovered_agents += discovered.agents.len();
        output.discovered_skills += discovered.skills.len();
        output.discovered_commands += discovered.commands.len();

        let installed =
            !agents_arr.is_empty() || !skills_arr.is_empty() || !commands_arr.is_empty();
        let has_discoverable = !discovered.agents.is_empty()
            || !discovered.skills.is_empty()
            || !discovered.commands.is_empty();
        if discovered.exists && has_discoverable && !installed {
            output.installable_plugins += 1;
        }

        output.plugins.push(json!({
            "name": name,
            "source": source,
            "category": category,
            "description": description,
            "version": version,
            "exists": discovered.exists,
            "installed": installed,
            "agents": agents_arr.len(),
            "skills": skills_arr.len(),
            "commands": commands_arr.len(),
            "counts": {
                "agents": {
                    "installed": agents_arr.len(),
                    "discoverable": discovered.agents.len()
                },
                "skills": {
                    "installed": skills_arr.len(),
                    "discoverable": discovered.skills.len()
                },
                "commands": {
                    "installed": commands_arr.len(),
                    "discoverable": discovered.commands.len()
                }
            },
            "entries": {
                "agents_installed": agents_arr,
                "skills_installed": skills_arr,
                "commands_installed": commands_arr,
                "agents_discoverable": discovered.agents,
                "skills_discoverable": discovered.skills,
                "commands_discoverable": discovered.commands
            }
        }));

        for path in agents_arr {
            let key = format!("{}::{}", source, path);
            if !seen_agent_keys.insert(key) {
                continue;
            }
            output.agents.push(json!({
                "kind": "marketplace",
                "id": format!("{}:{}", source, path),
                "name": display_name_from_entry(path.as_str()),
                "plugin": name,
                "source": source,
                "path": path
            }));
        }

        for path in skills_arr {
            let key = format!("{}::{}", source, path);
            if !seen_skill_keys.insert(key) {
                continue;
            }
            output.skills.push(json!({
                "id": format!("{}:{}", source, path),
                "name": display_name_from_entry(path.as_str()),
                "plugin": name,
                "source": source,
                "path": path
            }));
        }
    }

    output
}

fn discover_plugin_entries(plugins_root: &Path, source: &str) -> DiscoveredPluginEntries {
    let Some(plugin_root) = resolve_plugin_root(plugins_root, source) else {
        return DiscoveredPluginEntries::default();
    };

    DiscoveredPluginEntries {
        exists: true,
        agents: discover_agents_or_commands(plugin_root.as_path(), "agents"),
        skills: discover_skill_entries(plugin_root.as_path()),
        commands: discover_agents_or_commands(plugin_root.as_path(), "commands"),
    }
}

fn resolve_plugin_root(plugins_root: &Path, source: &str) -> Option<PathBuf> {
    plugin_source_candidates(source)
        .into_iter()
        .map(|rel| plugins_root.join(rel.as_str()))
        .find(|path| path.exists() && path.is_dir())
}

fn plugin_source_candidates(source: &str) -> Vec<String> {
    let normalized = normalize_plugin_source(source);
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut items = Vec::new();
    push_unique_string(&mut items, normalized.clone());

    if let Some(stripped) = normalized.strip_prefix("plugins/") {
        push_unique_string(&mut items, stripped.to_string());
    } else {
        push_unique_string(&mut items, format!("plugins/{}", normalized));
    }

    items
}

fn discover_agents_or_commands(plugin_root: &Path, dir_name: &str) -> Vec<String> {
    let root = plugin_root.join(dir_name);
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let mut items = collect_markdown_entries(root.as_path())
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| !name.eq_ignore_ascii_case("README.md"))
                .unwrap_or(true)
        })
        .filter_map(|path| path_to_unix_relative(plugin_root, path.as_path()))
        .collect::<Vec<_>>();

    items.sort();
    items
}

fn discover_skill_entries(plugin_root: &Path) -> Vec<String> {
    let root = plugin_root.join("skills");
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    for path in collect_markdown_entries(root.as_path()) {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        if file_name.eq_ignore_ascii_case("SKILL.md") || file_name.eq_ignore_ascii_case("index.md")
        {
            let parent = path.parent().unwrap_or_else(|| root.as_path());
            if let Some(rel) = path_to_unix_relative(plugin_root, parent) {
                if !rel.trim().is_empty() {
                    seen.insert(rel);
                }
            }
            continue;
        }

        if file_name.eq_ignore_ascii_case("README.md") {
            continue;
        }

        if contains_path_component(path.as_path(), "references") {
            continue;
        }

        let Some(parent) = path.parent() else {
            continue;
        };
        if parent != root.as_path() {
            continue;
        }

        if let Some(rel) = path_to_unix_relative(plugin_root, path.as_path()) {
            seen.insert(rel);
        }
    }

    let mut items = seen.into_iter().collect::<Vec<_>>();
    items.sort();
    items
}

fn collect_markdown_entries(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() || !root.is_dir() {
        return out;
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(dir.as_path()) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                if !is_skipped_repo_dir(path.as_path()) {
                    stack.push(path);
                }
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let is_markdown = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("md"))
                .unwrap_or(false);
            if is_markdown {
                out.push(path);
            }
        }
    }

    out
}

fn path_to_unix_relative(base: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(base).ok()?;
    let rendered = rel.to_string_lossy().replace('\\', "/");
    let trimmed = rendered.trim_matches('/').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn contains_path_component(path: &Path, target: &str) -> bool {
    path.components().any(|comp| {
        comp.as_os_str()
            .to_str()
            .map(|name| name.eq_ignore_ascii_case(target))
            .unwrap_or(false)
    })
}

fn display_name_from_entry(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty() {
        return String::new();
    }

    let parts = normalized
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return normalized;
    }

    let last = parts.last().copied().unwrap_or("");
    if last.eq_ignore_ascii_case("SKILL.md") || last.eq_ignore_ascii_case("index.md") {
        return parts
            .iter()
            .rev()
            .nth(1)
            .map(|part| part.to_string())
            .unwrap_or_else(|| last.to_string());
    }

    if let Some(stem) = last.strip_suffix(".md") {
        return stem.to_string();
    }

    last.to_string()
}

fn normalize_plugin_source(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

fn plugin_source_leaf(source: &str) -> String {
    let normalized = normalize_plugin_source(source);
    normalized
        .rsplit('/')
        .find(|part| !part.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_default()
}

fn plugin_match_key(value: &str) -> String {
    let normalized = normalize_plugin_source(value).replace('_', "-");
    let mut output = String::new();
    let mut last_dash = false;

    for ch in normalized.chars() {
        if ch.is_ascii_whitespace() {
            if !last_dash {
                output.push('-');
                last_dash = true;
            }
            continue;
        }

        if ch == '-' {
            if !last_dash {
                output.push('-');
                last_dash = true;
            }
            continue;
        }

        if ch == '/' {
            output.push('/');
            last_dash = false;
            continue;
        }

        output.push(ch.to_ascii_lowercase());
        last_dash = false;
    }

    output.trim_matches('-').to_string()
}

fn push_unique_string(items: &mut Vec<String>, value: String) {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return;
    }
    if !items.iter().any(|item| item == &trimmed) {
        items.push(trimmed);
    }
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

fn has_non_empty_string_array(value: Option<&Value>) -> bool {
    value
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|entry| entry.as_str())
                .any(|entry| !entry.trim().is_empty())
        })
        .unwrap_or(false)
}

fn value_to_string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|entry| entry.as_str())
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
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
