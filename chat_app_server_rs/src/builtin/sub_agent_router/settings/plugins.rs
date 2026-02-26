use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::state::{ensure_state_files, parse_registry_agents};
use super::types::{DiscoveredPluginEntries, InstallPluginOptions, ParsedMarketplaceItems};

pub(super) fn load_settings_summary() -> Result<Value, String> {
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

pub(super) fn install_plugins(opts: InstallPluginOptions) -> Result<Value, String> {
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

fn is_skipped_repo_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
        return false;
    };

    matches!(name, ".git" | "node_modules" | "target" | ".next")
}
