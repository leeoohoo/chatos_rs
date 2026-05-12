use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};
use serde_json::{json, Value};

use crate::core::time::now_rfc3339;
use crate::models::chatos_agent_types::{
    ChatosSkillDto, ChatosSkillPluginCommandDto, ChatosSkillPluginDto,
};
use crate::models::memory_skill::{
    MemorySkill, MemorySkillPlugin, MemorySkillPluginCommand,
};
use crate::repositories::memory_skills as skills_repo;

#[derive(Debug, Clone)]
struct SkillPluginCandidate {
    source: String,
    name: String,
    category: Option<String>,
    description: Option<String>,
    version: Option<String>,
}

pub struct ImportSkillsOutcome {
    pub repository: String,
    pub branch: Option<String>,
    pub imported_sources: Vec<String>,
    pub details: Vec<Value>,
}

pub async fn list_skills(
    user_id: &str,
    plugin_source: Option<&str>,
    query: Option<&str>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<ChatosSkillDto>, String> {
    let visible_user_ids = resolve_visible_user_ids(user_id);
    let mut items = skills_repo::list_skills(
        visible_user_ids.as_slice(),
        normalize_optional_text(plugin_source).as_deref(),
        normalize_optional_text(query).as_deref(),
        limit.unwrap_or(200),
        offset,
    )
    .await?;
    let discovered = discover_cached_skills(
        visible_user_ids.as_slice(),
        normalize_optional_text(plugin_source).as_deref(),
        normalize_optional_text(query).as_deref(),
    )?;
    merge_skills(&mut items, discovered);
    sort_skills_desc(&mut items);
    items = paginate_items(items, limit.unwrap_or(200), offset);
    Ok(items.into_iter().map(skill_to_dto).collect())
}

pub async fn get_skill(
    user_id: &str,
    skill_id: &str,
) -> Result<Option<ChatosSkillDto>, String> {
    let visible_user_ids = resolve_visible_user_ids(user_id);
    if let Some(item) = skills_repo::get_skill_by_id(visible_user_ids.as_slice(), skill_id).await? {
        return Ok(Some(skill_to_dto(item)));
    }

    let discovered = discover_cached_skills(visible_user_ids.as_slice(), None, None)?;
    Ok(discovered
        .into_iter()
        .find(|item| item.id == skill_id)
        .map(skill_to_dto))
}

pub async fn list_skill_plugins(
    user_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<ChatosSkillPluginDto>, String> {
    let visible_user_ids = resolve_visible_user_ids(user_id);
    let mut items = skills_repo::list_plugins_by_user_ids(
        visible_user_ids.as_slice(),
        limit.unwrap_or(200),
        offset,
    )
    .await?;
    let discovered = discover_cached_plugins(visible_user_ids.as_slice())?;
    merge_plugins(&mut items, discovered);
    sort_plugins_desc(&mut items);
    items = paginate_items(items, limit.unwrap_or(200), offset);
    Ok(items.into_iter().map(plugin_to_dto).collect())
}

pub async fn get_skill_plugin(
    user_id: &str,
    source: &str,
) -> Result<Option<ChatosSkillPluginDto>, String> {
    let normalized_source = normalize_plugin_source(source);
    if normalized_source.is_empty() {
        return Ok(None);
    }

    let visible_user_ids = resolve_visible_user_ids(user_id);
    let item =
        skills_repo::get_plugin_by_source_for_user_ids(visible_user_ids.as_slice(), normalized_source.as_str())
            .await?;
    let mut item = match item {
        Some(item) => item,
        None => {
            let discovered = discover_cached_plugins(visible_user_ids.as_slice())?;
            match discovered.into_iter().find(|item| {
                normalize_plugin_source(item.source.as_str()) == normalized_source
            }) {
                Some(item) => item,
                None => return Ok(None),
            }
        }
    };

    if plugin_needs_refresh(&item) {
        if let Some(refreshed) = hydrate_plugin_from_cache(&item).await? {
            item = refreshed;
        }
    }

    Ok(Some(plugin_to_dto(item)))
}

pub async fn import_skills_from_git(
    user_id: &str,
    repository: String,
    branch: Option<String>,
    marketplace_path: Option<String>,
    plugins_path: Option<String>,
) -> Result<ImportSkillsOutcome, String> {
    let state_root = resolve_skill_state_root(user_id);
    let plugins_root = state_root.join("plugins");
    let git_cache_root = state_root.join("git-cache");
    ensure_dir(plugins_root.as_path())?;
    ensure_dir(git_cache_root.as_path())?;

    let repo_root = tokio::task::spawn_blocking({
        let repository = repository.clone();
        let branch = branch.clone();
        let git_cache_root = git_cache_root.clone();
        move || ensure_git_repo(repository.as_str(), branch.as_deref(), git_cache_root.as_path())
    })
    .await
    .map_err(|err| format!("blocking task join failed: {}", err))??;

    let candidates = tokio::task::spawn_blocking({
        let repo_root = repo_root.clone();
        let marketplace_path = marketplace_path.clone();
        let plugins_path = plugins_path.clone();
        move || {
            load_plugin_candidates_from_repo(
                repo_root.as_path(),
                marketplace_path.as_deref(),
                plugins_path.as_deref(),
            )
        }
    })
    .await
    .map_err(|err| format!("blocking task join failed: {}", err))??;

    if candidates.is_empty() {
        return Err("no plugins discovered from repository".to_string());
    }

    let sources = candidates
        .iter()
        .map(|item| item.source.clone())
        .collect::<Vec<_>>();
    let existing = skills_repo::get_plugins_by_sources(user_id, &sources)
        .await
        .unwrap_or_default();
    let existing_by_source = existing
        .into_iter()
        .map(|item| (item.source.clone(), item))
        .collect::<HashMap<_, _>>();

    let mut imported_sources = Vec::new();
    let mut details = Vec::new();

    for candidate in candidates {
        let cache_rel = tokio::task::spawn_blocking({
            let repo_root = repo_root.clone();
            let plugins_root = plugins_root.clone();
            let source = candidate.source.clone();
            move || copy_plugin_source_from_repo(repo_root.as_path(), plugins_root.as_path(), source.as_str())
        })
        .await
        .map_err(|err| format!("blocking task join failed: {}", err))??;

        let plugin_root = plugins_root.join(cache_rel.as_str());
        let extracted = extract_plugin_content(plugin_root.as_path());
        let discoverable_skills = discover_skill_entries(plugin_root.as_path())
            .len()
            .min(i64::MAX as usize) as i64;
        let previous = existing_by_source.get(candidate.source.as_str());
        let extracted_name = read_plugin_name(plugin_root.as_path());
        let extracted_description = read_plugin_description(plugin_root.as_path());
        let extracted_version = read_plugin_version(plugin_root.as_path());
        let extracted_main_content = extracted
            .content
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned);
        let extracted_commands = extracted.commands;
        let plugin = MemorySkillPlugin {
            id: previous
                .map(|item| item.id.clone())
                .unwrap_or_else(|| hash_id(&["plugin", user_id, candidate.source.as_str()])),
            user_id: user_id.to_string(),
            source: candidate.source.clone(),
            name: candidate.name.clone(),
            category: candidate.category.clone(),
            description: candidate.description.clone().or(extracted_description),
            version: candidate.version.clone().or(extracted_version),
            repository: Some(repository.clone()),
            branch: branch.clone(),
            cache_path: Some(cache_rel.clone()),
            content: extracted_main_content,
            command_count: extracted_commands.len().min(i64::MAX as usize) as i64,
            commands: extracted_commands,
            installed: previous.map(|item| item.installed).unwrap_or(false),
            discoverable_skills,
            installed_skill_count: previous.map(|item| item.installed_skill_count).unwrap_or(0),
            updated_at: now_rfc3339(),
        };
        let plugin = if plugin.name.trim().is_empty() {
            MemorySkillPlugin {
                name: extracted_name.unwrap_or_else(|| candidate.source.clone()),
                ..plugin
            }
        } else {
            plugin
        };

        match skills_repo::upsert_plugin(plugin).await {
            Ok(saved) => {
                imported_sources.push(saved.source.clone());
                details.push(json!({
                    "source": saved.source,
                    "name": saved.name,
                    "discoverable_skills": saved.discoverable_skills,
                    "commands": saved.commands.len(),
                    "installed": saved.installed,
                    "cache_path": saved.cache_path,
                    "ok": true
                }));
            }
            Err(err) => {
                details.push(json!({
                    "source": candidate.source,
                    "ok": false,
                    "error": err
                }));
            }
        }
    }

    Ok(ImportSkillsOutcome {
        repository,
        branch,
        imported_sources,
        details,
    })
}

pub async fn list_all_plugin_sources(user_id: &str) -> Result<Vec<String>, String> {
    let mut items = list_skill_plugins(user_id, Some(500), 0).await?;
    items.sort_by(|left, right| left.source.cmp(&right.source));
    items.dedup_by(|left, right| left.source == right.source);
    Ok(items.into_iter().map(|item| item.source).collect())
}

pub async fn install_skill_plugins(user_id: &str, sources: &[String]) -> Result<Value, String> {
    let normalized_sources = unique_strings(
        sources
            .iter()
            .map(|item| normalize_plugin_source(item.as_str()))
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>(),
    );
    if normalized_sources.is_empty() {
        return Err("no plugin sources specified".to_string());
    }

    let plugins = skills_repo::get_plugins_by_sources(user_id, &normalized_sources).await?;
    if plugins.is_empty() {
        return Err("plugins not found".to_string());
    }

    let state_root = resolve_skill_state_root(user_id);
    let plugins_root = state_root.join("plugins");
    let mut installed = 0usize;
    let mut skipped = 0usize;
    let mut details = Vec::new();

    for plugin in plugins {
        let Some(plugin_root) = resolve_plugin_root_from_cache(
            plugins_root.as_path(),
            plugin.cache_path.as_deref(),
            plugin.source.as_str(),
        ) else {
            skipped += 1;
            details.push(json!({
                "source": plugin.source,
                "ok": false,
                "reason": "cached plugin path not found"
            }));
            continue;
        };

        let extracted = extract_plugin_content(plugin_root.as_path());
        let mut plugin_command_count = 0usize;
        let mut refreshed_plugin = plugin.clone();
        let extracted_content = extracted
            .content
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned);
        let plugin_has_main_content = extracted_content.is_some();
        if extracted_content.is_some() {
            refreshed_plugin.content = extracted_content;
        }
        if !extracted.commands.is_empty() {
            plugin_command_count = extracted.commands.len();
            refreshed_plugin.commands = extracted.commands;
        }
        refreshed_plugin.command_count = plugin_command_count.min(i64::MAX as usize) as i64;
        if refreshed_plugin.description.is_none() {
            refreshed_plugin.description = read_plugin_description(plugin_root.as_path());
        }
        if refreshed_plugin.version.is_none() {
            refreshed_plugin.version = read_plugin_version(plugin_root.as_path());
        }
        if let Some(name) = read_plugin_name(plugin_root.as_path()) {
            if refreshed_plugin.name.trim().is_empty()
                || refreshed_plugin.name.trim() == refreshed_plugin.source.trim()
            {
                refreshed_plugin.name = name;
            }
        }
        let _ = skills_repo::upsert_plugin(refreshed_plugin).await;

        let skills = build_skills_from_plugin(
            plugin_root.as_path(),
            user_id,
            plugin.source.as_str(),
            plugin.version.clone(),
        )?;
        let discoverable_count = skills.len().min(i64::MAX as usize) as i64;

        if discoverable_count <= 0 {
            let _ =
                skills_repo::replace_skills_for_plugin(user_id, plugin.source.as_str(), Vec::new())
                    .await;
            let _ =
                skills_repo::update_plugin_install_state(user_id, plugin.source.as_str(), 0, 0)
                    .await;
            if plugin_has_main_content || plugin_command_count > 0 {
                installed += 1;
                details.push(json!({
                    "source": plugin.source,
                    "ok": true,
                    "installed_skills": 0,
                    "commands": plugin_command_count,
                    "note": "no skills discovered; plugin content/commands still available"
                }));
            } else {
                skipped += 1;
                details.push(json!({
                    "source": plugin.source,
                    "ok": false,
                    "reason": "no skills discovered in plugin"
                }));
            }
            continue;
        }

        let installed_count =
            skills_repo::replace_skills_for_plugin(user_id, plugin.source.as_str(), skills).await?;
        let _ = skills_repo::update_plugin_install_state(
            user_id,
            plugin.source.as_str(),
            installed_count as i64,
            discoverable_count,
        )
        .await?;
        installed += 1;
        details.push(json!({
            "source": plugin.source,
            "ok": true,
            "installed_skills": installed_count
        }));
    }

    Ok(json!({
        "ok": true,
        "installed_plugins": installed,
        "skipped_plugins": skipped,
        "details": details
    }))
}

fn skill_to_dto(skill: MemorySkill) -> ChatosSkillDto {
    ChatosSkillDto {
        id: skill.id,
        user_id: skill.user_id,
        plugin_source: skill.plugin_source,
        name: skill.name,
        description: skill.description,
        content: skill.content,
        source_path: skill.source_path,
        version: skill.version,
        updated_at: skill.updated_at,
    }
}

fn plugin_to_dto(plugin: MemorySkillPlugin) -> ChatosSkillPluginDto {
    ChatosSkillPluginDto {
        id: plugin.id,
        user_id: plugin.user_id,
        source: plugin.source,
        name: plugin.name,
        category: plugin.category,
        description: plugin.description,
        version: plugin.version,
        repository: plugin.repository,
        branch: plugin.branch,
        cache_path: plugin.cache_path,
        content: plugin.content,
        commands: plugin
            .commands
            .into_iter()
            .map(|command| ChatosSkillPluginCommandDto {
                name: command.name,
                source_path: command.source_path,
                description: command.description,
                argument_hint: command.argument_hint,
                content: command.content,
            })
            .collect(),
        command_count: plugin.command_count,
        installed: plugin.installed,
        discoverable_skills: plugin.discoverable_skills,
        installed_skill_count: plugin.installed_skill_count,
        updated_at: plugin.updated_at,
    }
}

fn resolve_visible_user_ids(scope_user_id: &str) -> Vec<String> {
    let normalized = scope_user_id.trim();
    if normalized.is_empty() {
        Vec::new()
    } else {
        vec![normalized.to_string()]
    }
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_plugin_source(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

fn resolve_skill_state_root(user_id: &str) -> PathBuf {
    let user_segment = sanitize_user_segment(user_id);
    if let Ok(raw) = std::env::var("MEMORY_SKILL_STATE_ROOT") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(user_segment);
        }
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    home.join(".chatos")
        .join("memory_skill_center")
        .join(user_segment)
}

fn discover_cached_plugins(user_ids: &[String]) -> Result<Vec<MemorySkillPlugin>, String> {
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

fn discover_cached_skills(
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
                    skill.description.clone().unwrap_or_default().to_ascii_lowercase(),
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

fn sanitize_user_segment(value: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in value.trim().chars() {
        let normalized = if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            ch
        } else {
            '-'
        };
        if normalized == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        output.push(normalized);
    }
    let trimmed = output.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed
    }
}

fn resolve_plugin_root_from_cache(
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

fn normalize_repo_relative_path(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in values {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

fn merge_skills(target: &mut Vec<MemorySkill>, items: Vec<MemorySkill>) {
    let mut seen_ids = target.iter().map(|item| item.id.clone()).collect::<std::collections::HashSet<_>>();
    for item in items {
        if seen_ids.insert(item.id.clone()) {
            target.push(item);
        }
    }
}

fn merge_plugins(target: &mut Vec<MemorySkillPlugin>, items: Vec<MemorySkillPlugin>) {
    let mut seen_sources = target
        .iter()
        .map(|item| item.source.clone())
        .collect::<std::collections::HashSet<_>>();
    for item in items {
        if seen_sources.insert(item.source.clone()) {
            target.push(item);
        }
    }
}

fn sort_skills_desc(items: &mut [MemorySkill]) {
    items.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.name.cmp(&right.name))
    });
}

fn sort_plugins_desc(items: &mut [MemorySkillPlugin]) {
    items.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.name.cmp(&right.name))
    });
}

fn paginate_items<T>(items: Vec<T>, limit: i64, offset: i64) -> Vec<T> {
    let offset = offset.max(0) as usize;
    let limit = limit.max(1).min(5000) as usize;
    items.into_iter().skip(offset).take(limit).collect()
}

fn plugin_needs_refresh(plugin: &MemorySkillPlugin) -> bool {
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

async fn hydrate_plugin_from_cache(
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

    let extracted = tokio::task::spawn_blocking(move || extract_plugin_content(plugin_root.as_path()))
        .await
        .map_err(|err| format!("blocking task join failed: {}", err))?;

    let Some(refreshed) = merge_extracted_plugin_content(plugin, extracted.content, extracted.commands)
    else {
        return Ok(None);
    };
    Ok(Some(refreshed))
}

fn merge_extracted_plugin_content(
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

    if changed {
        Some(refreshed)
    } else {
        None
    }
}

#[derive(Default)]
struct ExtractedPluginContent {
    content: Option<String>,
    commands: Vec<MemorySkillPluginCommand>,
}

fn discover_plugin_roots(plugins_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut stack = vec![plugins_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(dir.as_path()) {
            Ok(entries) => entries,
            Err(err) => return Err(err.to_string()),
        };
        let mut children = Vec::new();
        let mut qualifies = false;
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
            if name.eq_ignore_ascii_case(".claude-plugin")
                || name.eq_ignore_ascii_case("skills")
                || name.eq_ignore_ascii_case("agents")
                || name.eq_ignore_ascii_case("commands")
            {
                qualifies = true;
            }
            if !is_skipped_repo_dir(path.as_path()) {
                children.push(path);
            }
        }
        if qualifies && dir != plugins_root {
            out.push(dir);
            continue;
        }
        stack.extend(children);
    }
    out.sort();
    Ok(out)
}

fn extract_plugin_content(plugin_root: &Path) -> ExtractedPluginContent {
    let mut extracted = ExtractedPluginContent::default();

    let agents_root = plugin_root.join("agents");
    let mut agent_sections = Vec::new();
    if agents_root.exists() && agents_root.is_dir() {
        let mut agent_files = collect_markdown_files(agents_root.as_path());
        agent_files.sort();
        for path in agent_files {
            let raw = match fs::read_to_string(path.as_path()) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let trimmed_raw = raw.trim();
            if trimmed_raw.is_empty() {
                continue;
            }
            let rel = path_to_unix_relative(plugin_root, path.as_path())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            let (metadata, body) = parse_markdown_metadata(trimmed_raw);
            let name = metadata_value(&metadata, &["name"])
                .map(ToOwned::to_owned)
                .or_else(|| first_markdown_heading(body).map(ToOwned::to_owned))
                .or_else(|| {
                    path.file_stem()
                        .and_then(|value| value.to_str())
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| rel.clone());
            let mut section = vec![format!("### {} ({})", name, rel)];
            if let Some(description) = metadata_value(&metadata, &["description"]) {
                section.push(format!("简介：{}", description));
            }
            let normalized_body = body.trim();
            if !normalized_body.is_empty() {
                section.push(normalized_body.to_string());
            } else {
                section.push(trimmed_raw.to_string());
            }
            agent_sections.push(section.join("\n"));
        }
    }
    if !agent_sections.is_empty() {
        extracted.content = Some(agent_sections.join("\n\n---\n\n"));
    }

    let commands_root = plugin_root.join("commands");
    if commands_root.exists() && commands_root.is_dir() {
        let mut command_files = collect_markdown_files(commands_root.as_path());
        command_files.sort();
        for path in command_files {
            let raw = match fs::read_to_string(path.as_path()) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let trimmed_raw = raw.trim();
            if trimmed_raw.is_empty() {
                continue;
            }
            let rel = path_to_unix_relative(plugin_root, path.as_path())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            let (metadata, body) = parse_markdown_metadata(trimmed_raw);
            let name = metadata_value(&metadata, &["name"])
                .map(ToOwned::to_owned)
                .or_else(|| first_markdown_heading(body).map(ToOwned::to_owned))
                .or_else(|| {
                    path.file_stem()
                        .and_then(|value| value.to_str())
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| rel.clone());
            let content = body.trim();
            extracted.commands.push(MemorySkillPluginCommand {
                name,
                source_path: rel,
                description: metadata_value(&metadata, &["description"]).map(ToOwned::to_owned),
                argument_hint: metadata_value(&metadata, &["argument-hint", "argument_hint"])
                    .map(ToOwned::to_owned),
                content: if content.is_empty() {
                    trimmed_raw.to_string()
                } else {
                    content.to_string()
                },
            });
        }
    }

    extracted
}

fn build_skills_from_plugin(
    plugin_root: &Path,
    user_id: &str,
    plugin_source: &str,
    plugin_version: Option<String>,
) -> Result<Vec<MemorySkill>, String> {
    let entries = discover_skill_entries(plugin_root);
    let mut skills = Vec::new();
    for entry in entries {
        let Some(file_path) = normalize_skill_entry_to_file(plugin_root, entry.as_str()) else {
            continue;
        };
        let raw = match fs::read_to_string(file_path.as_path()) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let trimmed_raw = raw.trim();
        if trimmed_raw.is_empty() {
            continue;
        }
        let (metadata, body) = parse_markdown_metadata(trimmed_raw);
        let normalized_body = body.trim();
        let content = if normalized_body.is_empty() {
            trimmed_raw.to_string()
        } else {
            normalized_body.to_string()
        };
        let id = hash_id(&["skill", user_id, plugin_source, entry.as_str()]);
        skills.push(MemorySkill {
            id,
            user_id: user_id.to_string(),
            plugin_source: plugin_source.to_string(),
            name: metadata_value(&metadata, &["name"])
                .map(ToOwned::to_owned)
                .or_else(|| first_markdown_heading(body).map(ToOwned::to_owned))
                .unwrap_or_else(|| build_skill_name_from_entry(entry.as_str())),
            description: metadata_value(&metadata, &["description"]).map(ToOwned::to_owned),
            content,
            source_path: entry,
            version: plugin_version.clone(),
            updated_at: now_rfc3339(),
        });
    }
    Ok(skills)
}

fn discover_skill_entries(plugin_root: &Path) -> Vec<String> {
    let root = plugin_root.join("skills");
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let mut seen = std::collections::HashSet::new();
    for path in collect_markdown_entries(root.as_path()) {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        if file_name.eq_ignore_ascii_case("README.md") {
            continue;
        }
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
        if contains_path_component(path.as_path(), "references") {
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

fn contains_path_component(path: &Path, target: &str) -> bool {
    path.components().any(|comp| {
        comp.as_os_str()
            .to_str()
            .map(|name| name.eq_ignore_ascii_case(target))
            .unwrap_or(false)
    })
}

fn normalize_skill_entry_to_file(plugin_root: &Path, entry: &str) -> Option<PathBuf> {
    let normalized = normalize_repo_relative_path(entry);
    if normalized.is_empty() {
        return None;
    }
    let path = plugin_root.join(normalized.as_str());
    if path.is_file() {
        return Some(path);
    }
    if path.is_dir() {
        let skill_md = path.join("SKILL.md");
        if skill_md.exists() && skill_md.is_file() {
            return Some(skill_md);
        }
        let index_md = path.join("index.md");
        if index_md.exists() && index_md.is_file() {
            return Some(index_md);
        }
    }
    None
}

fn build_skill_name_from_entry(entry: &str) -> String {
    let normalized = normalize_repo_relative_path(entry);
    if normalized.is_empty() {
        return "Skill".to_string();
    }
    let parts = normalized
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return "Skill".to_string();
    }
    let last = parts.last().copied().unwrap_or("");
    if last.eq_ignore_ascii_case("SKILL.md") || last.eq_ignore_ascii_case("index.md") {
        return parts
            .iter()
            .rev()
            .nth(1)
            .map(|value| (*value).to_string())
            .unwrap_or_else(|| "Skill".to_string());
    }
    if let Some(stem) = last.strip_suffix(".md") {
        return stem.to_string();
    }
    last.to_string()
}

fn collect_markdown_files(root: &Path) -> Vec<PathBuf> {
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

fn is_skipped_repo_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
        return false;
    };
    matches!(name, ".git" | "node_modules" | "target" | ".next")
}

fn read_plugin_json_value(plugin_root: &Path, key: &str) -> Option<String> {
    let plugin_json = plugin_root.join(".claude-plugin").join("plugin.json");
    let raw = fs::read_to_string(plugin_json.as_path()).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(raw.as_str()).ok()?;
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn read_plugin_name(plugin_root: &Path) -> Option<String> {
    read_plugin_json_value(plugin_root, "name")
}

fn read_plugin_description(plugin_root: &Path) -> Option<String> {
    read_plugin_json_value(plugin_root, "description")
}

fn read_plugin_version(plugin_root: &Path) -> Option<String> {
    read_plugin_json_value(plugin_root, "version")
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

fn parse_markdown_metadata(raw: &str) -> (HashMap<String, String>, &str) {
    parse_markdown_frontmatter(raw)
}

fn parse_markdown_frontmatter(raw: &str) -> (HashMap<String, String>, &str) {
    let mut out = HashMap::new();
    if !raw.starts_with("---\n") && !raw.starts_with("---\r\n") {
        return (out, raw);
    }
    let mut lines = raw.lines();
    let first = lines.next().unwrap_or_default();
    if first.trim() != "---" {
        return (out, raw);
    }

    let mut consumed = first.len();
    if raw.as_bytes().get(consumed) == Some(&b'\r') {
        consumed += 1;
    }
    if raw.as_bytes().get(consumed) == Some(&b'\n') {
        consumed += 1;
    }

    for line in lines {
        consumed += line.len();
        if raw.as_bytes().get(consumed) == Some(&b'\r') {
            consumed += 1;
        }
        if raw.as_bytes().get(consumed) == Some(&b'\n') {
            consumed += 1;
        }
        if line.trim() == "---" {
            let body = raw.get(consumed..).unwrap_or_default();
            return (out, body);
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = normalize_metadata_key(key);
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if key.is_empty() || value.is_empty() {
            continue;
        }
        out.insert(key, value.to_string());
    }
    (out, raw)
}

fn normalize_metadata_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

fn metadata_value<'a>(metadata: &'a HashMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        let normalized = normalize_metadata_key(key);
        if let Some(value) = metadata.get(normalized.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn first_markdown_heading(body: &str) -> Option<&str> {
    body.lines()
        .map(str::trim)
        .find(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim())
        .filter(|line| !line.is_empty())
}

fn hash_id(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    let mut out = String::new();
    for byte in digest {
        out.push_str(format!("{:02x}", byte).as_str());
    }
    out
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|err| err.to_string())
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
        fs::remove_dir_all(repo_path.as_path()).map_err(|err| {
            format!(
                "remove old repo failed ({}): {}",
                repo_path.to_string_lossy(),
                err
            )
        })?;
    }

    let mut args = vec!["clone".to_string(), "--depth".to_string(), "1".to_string()];
    if let Some(value) = branch {
        args.push("--branch".to_string());
        args.push(value.to_string());
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
        .map_err(|err| format!("git execution failed: {}", err))?;

    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!(
        "git command failed (exit={}): {}",
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

fn load_plugin_candidates_from_repo(
    repo_root: &Path,
    marketplace_path: Option<&str>,
    plugins_path: Option<&str>,
) -> Result<Vec<SkillPluginCandidate>, String> {
    if let Some(path) = marketplace_path
        .map(normalize_repo_relative_path)
        .filter(|value| !value.is_empty())
    {
        let file = repo_root.join(path.as_str());
        if !file.exists() || !file.is_file() {
            return Err(format!(
                "marketplace path not found: {}",
                file.to_string_lossy()
            ));
        }
        let raw = fs::read_to_string(file.as_path()).map_err(|err| err.to_string())?;
        let parsed = parse_marketplace_candidates(raw.as_str())?;
        if !parsed.is_empty() {
            return Ok(parsed);
        }
    } else if let Some(file) = find_default_file_recursively(repo_root, &["marketplace.json"]) {
        if let Ok(raw) = fs::read_to_string(file.as_path()) {
            let parsed = parse_marketplace_candidates(raw.as_str())?;
            if !parsed.is_empty() {
                return Ok(parsed);
            }
        }
    }
    Ok(fallback_plugin_candidates(repo_root, plugins_path))
}

fn parse_marketplace_candidates(raw: &str) -> Result<Vec<SkillPluginCandidate>, String> {
    let value = serde_json::from_str::<serde_json::Value>(raw)
        .map_err(|err| format!("marketplace json parse failed: {}", err))?;
    let plugins = value
        .get("plugins")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for item in plugins {
        let source = item
            .get("source")
            .and_then(serde_json::Value::as_str)
            .map(normalize_plugin_source)
            .unwrap_or_default();
        if source.is_empty() || has_parent_path_component(source.as_str()) {
            continue;
        }
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| source.clone());
        let category = item
            .get("category")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let description = item
            .get("description")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let version = item
            .get("version")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        out.push(SkillPluginCandidate {
            source,
            name,
            category,
            description,
            version,
        });
    }
    Ok(unique_plugin_candidates(out))
}

fn fallback_plugin_candidates(repo_root: &Path, plugins_path: Option<&str>) -> Vec<SkillPluginCandidate> {
    let root = plugins_path
        .map(normalize_repo_relative_path)
        .filter(|value| !value.is_empty())
        .map(|value| repo_root.join(value))
        .unwrap_or_else(|| repo_root.join("plugins"));
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }
    let entries = match fs::read_dir(root.as_path()) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let rel = path_to_unix_relative(repo_root, path.as_path());
        let Some(rel) = rel else {
            continue;
        };
        let source = normalize_plugin_source(rel.as_str());
        if source.is_empty() || has_parent_path_component(source.as_str()) {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| source.clone());
        out.push(SkillPluginCandidate {
            source,
            name,
            category: None,
            description: None,
            version: None,
        });
    }
    unique_plugin_candidates(out)
}

fn unique_plugin_candidates(items: Vec<SkillPluginCandidate>) -> Vec<SkillPluginCandidate> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if seen.insert(item.source.clone()) {
            out.push(item);
        }
    }
    out
}

fn find_default_file_recursively(root: &Path, names: &[&str]) -> Option<PathBuf> {
    let target_names = names
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(dir.as_path()).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = entry.file_type().ok()?;
            if file_type.is_dir() {
                if !is_skipped_repo_dir(path.as_path()) {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let name = path.file_name()?.to_str()?.to_ascii_lowercase();
            if target_names.contains(name.as_str()) {
                return Some(path);
            }
        }
    }
    None
}

fn has_parent_path_component(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn copy_plugin_source_from_repo(
    repo_root: &Path,
    plugins_root: &Path,
    source: &str,
) -> Result<String, String> {
    let normalized = normalize_plugin_source(source);
    if normalized.is_empty() {
        return Err("plugin source is empty".to_string());
    }
    if has_parent_path_component(normalized.as_str()) {
        return Err("plugin source cannot contain ..".to_string());
    }
    let src = repo_root.join(normalized.as_str());
    if !src.exists() {
        return Err(format!(
            "plugin source not found in repository: {}",
            normalized
        ));
    }
    let dest_rel = plugin_install_destination(normalized.as_str());
    if dest_rel.is_empty() {
        return Err("plugin source normalization failed".to_string());
    }
    let dest = plugins_root.join(dest_rel.as_str());
    copy_path(src.as_path(), dest.as_path())?;
    Ok(dest_rel)
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

fn plugin_install_destination(source: &str) -> String {
    let normalized = normalize_plugin_source(source);
    if let Some(stripped) = normalized.strip_prefix("plugins/") {
        stripped.trim_matches('/').to_string()
    } else {
        normalized
    }
}
