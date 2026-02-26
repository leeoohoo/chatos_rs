use std::fs;
use std::path::{Path, PathBuf};

use crate::builtin::sub_agent_router::types::{AgentSpec, CommandSpec, SkillSpec};

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

    let mut agents = Vec::new();
    let mut skills = Vec::new();
    let mut skill_ids = std::collections::HashSet::new();

    let marketplace_dir = marketplace_path.parent().unwrap_or_else(|| Path::new("."));

    for plugin in plugins {
        let source = plugin.source.unwrap_or_default().trim().to_string();
        if source.is_empty() {
            continue;
        }

        let plugin_root = resolve_plugin_root(source.as_str(), marketplace_dir, plugins_root);
        if !plugin_root.exists() {
            continue;
        }

        let plugin_name = plugin.name.unwrap_or_default();
        let plugin_scope =
            resolve_plugin_scope(plugin_root.as_path(), source.as_str(), plugin_name.as_str());
        let plugin_category = plugin
            .category
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty());

        let command_specs = build_command_specs(&plugin_root, plugin.commands.unwrap_or_default());
        let skill_specs = build_skill_specs(
            &plugin_root,
            plugin.skills.unwrap_or_default(),
            plugin_name.clone(),
        );

        for skill in &skill_specs {
            if skill_ids.insert(skill.id.clone()) {
                skills.push(skill.clone());
            }
        }
        let skill_ids_for_plugin: Vec<String> = skill_specs.iter().map(|s| s.id.clone()).collect();

        for agent_path in plugin.agents.unwrap_or_default() {
            let resolved = resolve_markdown_path(&plugin_root, agent_path.as_str());
            if !resolved.exists() {
                continue;
            }
            let meta = read_markdown_meta(&resolved);
            let local_id = derive_id(&resolved);
            let id = build_agent_id(plugin_scope.as_str(), local_id.as_str());
            agents.push(AgentSpec {
                id: id.clone(),
                name: if meta.title.is_empty() {
                    if local_id.is_empty() {
                        id.clone()
                    } else {
                        local_id
                    }
                } else {
                    meta.title
                },
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
                plugin: if plugin_name.is_empty() {
                    None
                } else {
                    Some(plugin_name.clone())
                },
            });
        }
    }

    MarketplaceResult { agents, skills }
}

fn resolve_plugin_root(
    source: &str,
    marketplace_dir: &Path,
    plugins_root: Option<&Path>,
) -> PathBuf {
    let source_path = Path::new(source);
    if source_path.is_absolute() {
        return source_path.to_path_buf();
    }
    if let Some(root) = plugins_root {
        if !root.as_os_str().is_empty() {
            let normalized = normalize_plugin_source_for_root(source);
            if !normalized.is_empty() {
                return root.join(normalized);
            }
            return root.to_path_buf();
        }
    }
    marketplace_dir.join(source)
}

fn normalize_plugin_source_for_root(source: &str) -> String {
    let mut normalized = source.trim().replace('\\', "/");
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    if let Some(rest) = normalized.strip_prefix("plugins/") {
        normalized = rest.to_string();
    }
    normalized.trim_matches('/').to_string()
}

fn resolve_plugin_scope(plugin_root: &Path, source: &str, plugin_name: &str) -> String {
    if let Some(folder_name) = plugin_root.file_name().and_then(|name| name.to_str()) {
        let slug = slugify(folder_name);
        if !slug.is_empty() {
            return slug;
        }
    }

    let normalized_source = normalize_plugin_source_for_root(source);
    if let Some(last_segment) = normalized_source.rsplit('/').next() {
        let slug = slugify(last_segment);
        if !slug.is_empty() {
            return slug;
        }
    }

    let slug = slugify(plugin_name);
    if !slug.is_empty() {
        return slug;
    }

    "plugin".to_string()
}

fn build_agent_id(plugin_scope: &str, local_id: &str) -> String {
    let local = slugify(local_id);
    if local.is_empty() {
        return plugin_scope.to_string();
    }

    if plugin_scope.trim().is_empty() {
        return local;
    }

    format!("{}/{}", plugin_scope.trim(), local)
}

fn build_command_specs(root: &Path, entries: Vec<String>) -> Vec<CommandSpec> {
    let mut specs = Vec::new();
    for entry in entries {
        let resolved = resolve_markdown_path(root, entry.as_str());
        if !resolved.exists() {
            continue;
        }
        let meta = read_markdown_meta(&resolved);
        let id = derive_id(&resolved);
        specs.push(CommandSpec {
            id: id.clone(),
            name: if meta.title.is_empty() {
                None
            } else {
                Some(meta.title)
            },
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
        let resolved = resolve_markdown_path(root, entry.as_str());
        if !resolved.exists() {
            continue;
        }
        let meta = read_markdown_meta(&resolved);
        let id = derive_id(&resolved);
        specs.push(SkillSpec {
            id: id.clone(),
            name: if meta.title.is_empty() {
                id
            } else {
                meta.title
            },
            description: if meta.description.is_empty() {
                None
            } else {
                Some(meta.description)
            },
            path: resolved.to_string_lossy().to_string(),
            plugin: if plugin.is_empty() {
                None
            } else {
                Some(plugin.clone())
            },
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

    let (frontmatter, content_lines) = parse_frontmatter(&text);

    let mut title = frontmatter
        .get("name")
        .cloned()
        .or_else(|| frontmatter.get("title").cloned())
        .unwrap_or_default();
    let mut description = frontmatter.get("description").cloned().unwrap_or_default();
    let mut found_title = !title.is_empty();

    for line in content_lines {
        let trimmed = line.trim();
        if !found_title && trimmed.starts_with('#') {
            title = trimmed.trim_start_matches('#').trim().to_string();
            found_title = true;
            continue;
        }

        if found_title && description.is_empty() && !trimmed.is_empty() && !trimmed.starts_with('#')
        {
            description = trimmed.to_string();
            break;
        }
    }

    MarkdownMeta { title, description }
}

fn parse_frontmatter<'a>(
    text: &'a str,
) -> (std::collections::HashMap<String, String>, Vec<&'a str>) {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.first().map(|line| line.trim()) != Some("---") {
        return (std::collections::HashMap::new(), lines);
    }

    let Some(frontmatter_end) = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(idx, line)| (line.trim() == "---").then_some(idx))
    else {
        return (std::collections::HashMap::new(), lines);
    };

    let mut frontmatter = std::collections::HashMap::new();
    for line in &lines[1..frontmatter_end] {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };

        let normalized_key = key.trim().to_lowercase();
        if normalized_key.is_empty() {
            continue;
        }

        let cleaned_value = cleanup_frontmatter_value(value);
        if cleaned_value.is_empty() {
            continue;
        }

        frontmatter.insert(normalized_key, cleaned_value);
    }

    let body = lines[(frontmatter_end + 1)..].to_vec();
    (frontmatter, body)
}

fn cleanup_frontmatter_value(raw: &str) -> String {
    let mut value = raw.trim().to_string();
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        if value.len() >= 2 {
            value = value[1..value.len() - 1].to_string();
        }
    }
    value.trim().to_string()
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
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
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

#[cfg(test)]
mod tests {
    use super::{load_marketplace, resolve_plugin_root};
    use std::path::{Path, PathBuf};

    #[test]
    fn resolves_dot_plugins_source_under_plugins_root() {
        let marketplace_dir = Path::new("C:/state");
        let plugins_root = Path::new("C:/state/plugins");

        let resolved = resolve_plugin_root(
            "./plugins/code-documentation",
            marketplace_dir,
            Some(plugins_root),
        );

        assert_eq!(
            resolved,
            PathBuf::from("C:/state/plugins/code-documentation")
        );
    }

    #[test]
    fn resolves_plugins_prefixed_source_under_plugins_root() {
        let marketplace_dir = Path::new("C:/state");
        let plugins_root = Path::new("C:/state/plugins");

        let resolved = resolve_plugin_root(
            "plugins/code-documentation",
            marketplace_dir,
            Some(plugins_root),
        );

        assert_eq!(
            resolved,
            PathBuf::from("C:/state/plugins/code-documentation")
        );
    }

    #[test]
    fn resolves_plain_relative_source_under_plugins_root() {
        let marketplace_dir = Path::new("C:/state");
        let plugins_root = Path::new("C:/state/plugins");

        let resolved =
            resolve_plugin_root("code-documentation", marketplace_dir, Some(plugins_root));

        assert_eq!(
            resolved,
            PathBuf::from("C:/state/plugins/code-documentation")
        );
    }

    #[test]
    fn resolves_windows_style_plugins_source_under_plugins_root() {
        let marketplace_dir = Path::new("C:/state");
        let plugins_root = Path::new("C:/state/plugins");

        let resolved = resolve_plugin_root(
            r".\\plugins\\code-documentation",
            marketplace_dir,
            Some(plugins_root),
        );

        assert_eq!(
            resolved,
            PathBuf::from("C:/state/plugins/code-documentation")
        );
    }

    #[test]
    fn keeps_marketplace_relative_path_without_plugins_root() {
        let marketplace_dir = Path::new("C:/state");

        let resolved = resolve_plugin_root("./plugins/code-documentation", marketplace_dir, None);

        assert_eq!(
            resolved,
            PathBuf::from("C:/state/plugins/code-documentation")
        );
    }

    #[test]
    fn loads_agents_from_dot_plugins_source_with_plugins_root() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("sub_agent_router_marketplace_{unique}"));
        let plugins_root = root.join("plugins");
        let plugin_root = plugins_root.join("code-documentation");
        let agents_dir = plugin_root.join("agents");

        fs::create_dir_all(&agents_dir).expect("create agents dir");
        fs::write(
            agents_dir.join("code-reviewer.md"),
            "---
name: code-reviewer
---
",
        )
        .expect("write agent file");

        let marketplace_path = root.join("marketplace.json");
        fs::write(
            &marketplace_path,
            r#"{"plugins":[{"name":"code-documentation","source":"./plugins/code-documentation","agents":["agents/code-reviewer.md"],"skills":[],"commands":[]}]}"#,
        )
        .expect("write marketplace file");

        let result = load_marketplace(marketplace_path.as_path(), Some(plugins_root.as_path()));
        assert_eq!(result.agents.len(), 1);
        assert_eq!(result.agents[0].id, "code-documentation/code-reviewer");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prefers_frontmatter_name_and_description_for_agent_meta() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("sub_agent_router_frontmatter_{unique}"));
        let plugins_root = root.join("plugins");
        let plugin_root = plugins_root.join("comprehensive-review");
        let agents_dir = plugin_root.join("agents");

        fs::create_dir_all(&agents_dir).expect("create agents dir");
        fs::write(
            agents_dir.join("code-reviewer.md"),
            "---
name: code-reviewer
description: security and reliability reviewer
---

# Expert Purpose

This heading should not override frontmatter.
",
        )
        .expect("write agent file");

        let marketplace_path = root.join("marketplace.json");
        fs::write(
            &marketplace_path,
            r#"{"plugins":[{"name":"comprehensive-review","source":"./plugins/comprehensive-review","agents":["agents/code-reviewer.md"],"skills":[],"commands":[]}]}"#,
        )
        .expect("write marketplace file");

        let result = load_marketplace(marketplace_path.as_path(), Some(plugins_root.as_path()));
        assert_eq!(result.agents.len(), 1);
        assert_eq!(result.agents[0].id, "comprehensive-review/code-reviewer");
        assert_eq!(result.agents[0].name, "code-reviewer");
        assert_eq!(
            result.agents[0].description.as_deref(),
            Some("security and reliability reviewer")
        );

        let _ = fs::remove_dir_all(root);
    }
}
