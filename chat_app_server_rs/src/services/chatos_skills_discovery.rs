use std::path::{Path, PathBuf};

use crate::core::time::now_rfc3339;
use crate::models::memory_skill::{MemorySkill, MemorySkillPlugin, MemorySkillPluginCommand};

use super::chatos_skills_helpers::{
    hash_id, normalize_plugin_source, normalize_repo_relative_path, path_to_unix_relative,
    resolve_skill_state_root, sort_plugins_desc, sort_skills_desc, unique_strings,
};
use super::chatos_skills_manifest::{
    build_skills_from_plugin, discover_plugin_roots, discover_skill_entries,
    extract_plugin_content, read_plugin_description, read_plugin_name, read_plugin_version,
};

pub fn discover_cached_plugins(user_ids: &[String]) -> Result<Vec<MemorySkillPlugin>, String> {
    let mut out = Vec::new();
    let mut seen_sources = std::collections::HashSet::new();
    for user_id in user_ids {
        for plugin in discover_cached_plugins_for_user(user_id)? {
            if seen_sources.insert(plugin.source.clone()) {
                out.push(plugin);
            }
        }
    }
    Ok(out)
}

pub fn discover_cached_skills(
    user_ids: &[String],
    plugin_source: Option<&str>,
    query: Option<&str>,
) -> Result<Vec<MemorySkill>, String> {
    let mut out = Vec::new();
    let target_plugin_source = plugin_source.map(normalize_plugin_source);
    let normalized_query = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    for plugin in discover_cached_plugins(user_ids)? {
        if let Some(target) = target_plugin_source.as_deref() {
            let source_norm = normalize_plugin_source(plugin.source.as_str());
            if source_norm != target {
                continue;
            }
        }
        let plugin_root = resolve_skill_state_root(plugin.user_id.as_str())
            .join("plugins")
            .join(plugin.cache_path.clone().unwrap_or_default());
        for skill in build_skills_from_plugin(
            plugin_root.as_path(),
            plugin.user_id.as_str(),
            plugin.source.as_str(),
            plugin.version.clone(),
        )? {
            if let Some(q) = normalized_query.as_deref() {
                let haystacks = [
                    skill.name.to_ascii_lowercase(),
                    skill
                        .description
                        .clone()
                        .unwrap_or_default()
                        .to_ascii_lowercase(),
                    skill.source_path.to_ascii_lowercase(),
                ];
                if !haystacks.iter().any(|item| item.contains(q)) {
                    continue;
                }
            }
            out.push(skill);
        }
    }
    sort_skills_desc(&mut out);
    Ok(out)
}

pub fn plugin_needs_refresh(plugin: &MemorySkillPlugin) -> bool {
    let content_missing = plugin
        .content
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .is_none();
    let commands_missing = plugin.commands.is_empty();
    let command_metadata_missing = plugin.commands.iter().any(|command| {
        let description_missing = command
            .description
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .is_none();
        let argument_hint_missing = command
            .argument_hint
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .is_none();
        description_missing || argument_hint_missing
    });
    content_missing || commands_missing || command_metadata_missing
}

pub async fn hydrate_plugin_from_cache(
    plugin: &MemorySkillPlugin,
) -> Result<Option<MemorySkillPlugin>, String> {
    let plugins_root = resolve_skill_state_root(plugin.user_id.as_str()).join("plugins");
    let Some(plugin_root) = resolve_plugin_root_from_cache(
        plugins_root.as_path(),
        plugin.cache_path.as_deref(),
        plugin.source.as_str(),
    ) else {
        return Ok(None);
    };

    let extracted =
        tokio::task::spawn_blocking(move || extract_plugin_content(plugin_root.as_path()))
            .await
            .map_err(|err| format!("blocking task join failed: {}", err))?;

    let Some(refreshed) =
        merge_extracted_plugin_content(plugin, extracted.content, extracted.commands)
    else {
        return Ok(None);
    };
    Ok(Some(refreshed))
}

pub fn resolve_plugin_root_from_cache(
    plugins_root: &Path,
    cache_path: Option<&str>,
    source: &str,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = cache_path
        .map(normalize_repo_relative_path)
        .filter(|value| !value.is_empty())
    {
        candidates.push(value);
    }
    let normalized = normalize_plugin_source(source);
    if !normalized.is_empty() {
        candidates.push(normalized.clone());
        if let Some(stripped) = normalized.strip_prefix("plugins/") {
            candidates.push(stripped.to_string());
        } else {
            candidates.push(format!("plugins/{}", normalized));
        }
    }
    for rel in unique_strings(candidates) {
        let path = plugins_root.join(rel.as_str());
        if path.exists() && path.is_dir() {
            return Some(path);
        }
    }
    None
}

pub fn merge_extracted_plugin_content(
    existing: &MemorySkillPlugin,
    extracted_content: Option<String>,
    extracted_commands: Vec<MemorySkillPluginCommand>,
) -> Option<MemorySkillPlugin> {
    let mut refreshed = existing.clone();
    let mut changed = false;

    let content_missing = refreshed
        .content
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .is_none();
    if content_missing {
        if let Some(content) = extracted_content
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            refreshed.content = Some(content.to_string());
            changed = true;
        }
    }

    let commands_missing = refreshed.commands.is_empty();
    let command_metadata_missing = refreshed.commands.iter().any(|command| {
        let description_missing = command
            .description
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .is_none();
        let argument_hint_missing = command
            .argument_hint
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .is_none();
        description_missing || argument_hint_missing
    });
    if (commands_missing || command_metadata_missing) && !extracted_commands.is_empty() {
        refreshed.commands = extracted_commands;
        refreshed.command_count = refreshed.commands.len().min(i64::MAX as usize) as i64;
        refreshed.updated_at = now_rfc3339();
        changed = true;
    }

    if changed { Some(refreshed) } else { None }
}

fn discover_cached_plugins_for_user(user_id: &str) -> Result<Vec<MemorySkillPlugin>, String> {
    let plugins_root = resolve_skill_state_root(user_id).join("plugins");
    if !plugins_root.exists() || !plugins_root.is_dir() {
        return Ok(Vec::new());
    }

    let roots = discover_plugin_roots(plugins_root.as_path())?;
    let mut out = Vec::new();
    for plugin_root in roots {
        let Some(rel) = path_to_unix_relative(plugins_root.as_path(), plugin_root.as_path()) else {
            continue;
        };
        let cache_path = normalize_repo_relative_path(rel.as_str());
        if cache_path.is_empty() {
            continue;
        }
        let source = format!("plugins/{}", cache_path);
        let extracted = extract_plugin_content(plugin_root.as_path());
        let discoverable_skills = discover_skill_entries(plugin_root.as_path())
            .len()
            .min(i64::MAX as usize) as i64;
        let content = extracted
            .content
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let command_count = extracted.commands.len().min(i64::MAX as usize) as i64;
        let mut name = plugin_root
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| source.clone());
        if let Some(candidate_name) = read_plugin_name(plugin_root.as_path()) {
            name = candidate_name;
        }
        out.push(MemorySkillPlugin {
            id: hash_id(&["plugin", user_id, source.as_str()]),
            user_id: user_id.to_string(),
            source,
            name,
            category: None,
            description: read_plugin_description(plugin_root.as_path()),
            version: read_plugin_version(plugin_root.as_path()),
            repository: None,
            branch: None,
            cache_path: Some(cache_path),
            content,
            commands: extracted.commands,
            command_count,
            installed: discoverable_skills > 0 || command_count > 0,
            discoverable_skills,
            installed_skill_count: discoverable_skills,
            updated_at: now_rfc3339(),
        });
    }
    sort_plugins_desc(&mut out);
    Ok(out)
}
