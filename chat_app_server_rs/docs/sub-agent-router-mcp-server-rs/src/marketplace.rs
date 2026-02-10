use crate::types::{AgentSpec, CommandSpec, SkillSpec};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Deserialize)]
struct MarketplaceFile {
    plugins: Option<Vec<PluginEntry>>,
}

#[derive(Debug, serde::Deserialize)]
struct PluginEntry {
    name: Option<String>,
    source: Option<String>,
    category: Option<String>,
    description: Option<String>,
    agents: Option<Vec<String>>,
    commands: Option<Vec<String>>,
    skills: Option<Vec<String>>,
}

pub struct MarketplaceResult {
    pub agents: Vec<AgentSpec>,
    pub skills: Vec<SkillSpec>,
}

pub fn load_marketplace(marketplace_path: &Path, plugins_root: Option<&Path>) -> MarketplaceResult {
    if !marketplace_path.exists() {
        return MarketplaceResult {
            agents: Vec::new(),
            skills: Vec::new(),
        };
    }
    let raw = match fs::read_to_string(marketplace_path) {
        Ok(text) => text,
        Err(_) => {
            return MarketplaceResult {
                agents: Vec::new(),
                skills: Vec::new(),
            }
        }
    };
    let parsed: MarketplaceFile = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => {
            return MarketplaceResult {
                agents: Vec::new(),
                skills: Vec::new(),
            }
        }
    };
    let plugins = parsed.plugins.unwrap_or_default();
    if plugins.is_empty() {
        return MarketplaceResult {
            agents: Vec::new(),
            skills: Vec::new(),
        };
    }

    let mut agents: Vec<AgentSpec> = Vec::new();
    let mut skills: Vec<SkillSpec> = Vec::new();
    let mut skill_ids = std::collections::HashSet::new();

    let marketplace_dir = marketplace_path.parent().unwrap_or_else(|| Path::new("."));

    for plugin in plugins {
        let source = plugin.source.unwrap_or_default().trim().to_string();
        if source.is_empty() {
            continue;
        }
        let plugin_root = resolve_plugin_root(&source, marketplace_dir, plugins_root);
        if !plugin_root.exists() {
            continue;
        }
        let plugin_name = plugin.name.unwrap_or_default();
        let plugin_category = plugin.category.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
        let command_specs = build_command_specs(&plugin_root, plugin.commands.unwrap_or_default());
        let skill_specs = build_skill_specs(&plugin_root, plugin.skills.unwrap_or_default(), plugin_name.clone());
        for skill in &skill_specs {
            if skill_ids.insert(skill.id.clone()) {
                skills.push(skill.clone());
            }
        }
        let skill_ids_for_plugin: Vec<String> = skill_specs.iter().map(|s| s.id.clone()).collect();
        for agent_path in plugin.agents.unwrap_or_default() {
            let resolved = resolve_markdown_path(&plugin_root, &agent_path);
            if !resolved.exists() {
                continue;
            }
            let meta = read_markdown_meta(&resolved);
            let id = derive_id(&resolved);
            agents.push(AgentSpec {
                id: id.clone(),
                name: if meta.title.is_empty() { id } else { meta.title },
                description: if !meta.description.is_empty() {
                    Some(meta.description)
                } else {
                    plugin.description.clone()
                },
                category: plugin_category.clone(),
                skills: Some(skill_ids_for_plugin.clone()),
                default_skills: Some(skill_ids_for_plugin.clone()),
                commands: Some(command_specs.clone()),
                default_command: command_specs.first().map(|c| c.id.clone()),
                system_prompt_path: Some(resolved.to_string_lossy().to_string()),
                plugin: if plugin_name.is_empty() { None } else { Some(plugin_name.clone()) },
            });
        }
    }

    MarketplaceResult { agents, skills }
}

fn resolve_plugin_root(source: &str, marketplace_dir: &Path, plugins_root: Option<&Path>) -> PathBuf {
    let source_path = Path::new(source);
    if source_path.is_absolute() {
        return source_path.to_path_buf();
    }
    if let Some(root) = plugins_root {
        if !root.as_os_str().is_empty() {
            return root.join(source);
        }
    }
    marketplace_dir.join(source)
}

fn build_command_specs(root: &Path, entries: Vec<String>) -> Vec<CommandSpec> {
    let mut specs = Vec::new();
    for entry in entries {
        let resolved = resolve_markdown_path(root, &entry);
        if !resolved.exists() {
            continue;
        }
        let meta = read_markdown_meta(&resolved);
        let id = derive_id(&resolved);
        specs.push(CommandSpec {
            id: id.clone(),
            name: if meta.title.is_empty() { None } else { Some(meta.title) },
            description: if meta.description.is_empty() {
                None
            } else {
                Some(meta.description)
            },
            exec: None,
            cwd: None,
            env: None,
            instructions_path: Some(resolved.to_string_lossy().to_string()),
        });
    }
    specs
}

fn build_skill_specs(root: &Path, entries: Vec<String>, plugin: String) -> Vec<SkillSpec> {
    let mut specs = Vec::new();
    for entry in entries {
        let resolved = resolve_markdown_path(root, &entry);
        if !resolved.exists() {
            continue;
        }
        let meta = read_markdown_meta(&resolved);
        let id = derive_id(&resolved);
        specs.push(SkillSpec {
            id: id.clone(),
            name: if meta.title.is_empty() { id } else { meta.title },
            description: if meta.description.is_empty() {
                None
            } else {
                Some(meta.description)
            },
            path: resolved.to_string_lossy().to_string(),
            plugin: if plugin.is_empty() { None } else { Some(plugin.clone()) },
        });
    }
    specs
}

fn resolve_markdown_path(root: &Path, raw_path: &str) -> PathBuf {
    if raw_path.trim().is_empty() {
        return root.to_path_buf();
    }
    let candidate = Path::new(raw_path);
    let resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    if resolved.exists() {
        return resolved;
    }
    if resolved.extension().is_none() {
        let with_md = resolved.with_extension("md");
        if with_md.exists() {
            return with_md;
        }
        let with_skill = resolved.join("SKILL.md");
        if with_skill.exists() {
            return with_skill;
        }
        let with_index = resolved.join("index.md");
        if with_index.exists() {
            return with_index;
        }
    }
    resolved
}

struct MarkdownMeta {
    title: String,
    description: String,
}

fn read_markdown_meta(path: &Path) -> MarkdownMeta {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            return MarkdownMeta {
                title: String::new(),
                description: String::new(),
            }
        }
    };
    let mut title = String::new();
    let mut description = String::new();
    let mut found_title = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !found_title && trimmed.starts_with('#') {
            title = trimmed.trim_start_matches('#').trim().to_string();
            found_title = true;
            continue;
        }
        if found_title && description.is_empty() && !trimmed.is_empty() && !trimmed.starts_with('#') {
            description = trimmed.to_string();
            break;
        }
    }
    MarkdownMeta { title, description }
}

fn derive_id(path: &Path) -> String {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let lower = file_name.to_lowercase();
    let raw = if lower == "skill.md" || lower == "index.md" {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    } else {
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
    };
    slugify(&raw)
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().to_lowercase().chars() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if valid {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}
