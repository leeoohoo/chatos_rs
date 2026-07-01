// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::core::time::now_rfc3339;
use crate::models::chatos_agent_types::{
    ChatosSkillDto, ChatosSkillPluginCommandDto, ChatosSkillPluginDto,
};
use crate::models::memory_skill::{MemorySkill, MemorySkillPlugin};
use crate::repositories::memory_skills as skills_repo;

use super::chatos_skills_discovery::{
    discover_cached_plugins, discover_cached_skills, hydrate_plugin_from_cache,
    plugin_needs_refresh, resolve_plugin_root_from_cache,
};
use super::chatos_skills_git::{copy_plugin_source_from_repo, ensure_git_repo};
use super::chatos_skills_helpers::{
    hash_id, merge_plugins, merge_skills, normalize_optional_text, normalize_plugin_source,
    paginate_items, resolve_skill_state_root, resolve_visible_user_ids, sort_plugins_desc,
    sort_skills_desc, unique_strings,
};
use super::chatos_skills_import::load_plugin_candidates_from_repo;
use super::chatos_skills_manifest::{
    build_skills_from_plugin, discover_skill_entries, extract_plugin_content,
    read_plugin_description, read_plugin_name, read_plugin_version,
};
use super::chatos_skills_types::ImportSkillsOutcome;

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

pub async fn get_skill(user_id: &str, skill_id: &str) -> Result<Option<ChatosSkillDto>, String> {
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
    let item = skills_repo::get_plugin_by_source_for_user_ids(
        visible_user_ids.as_slice(),
        normalized_source.as_str(),
    )
    .await?;
    let mut item = match item {
        Some(item) => item,
        None => {
            let discovered = discover_cached_plugins(visible_user_ids.as_slice())?;
            match discovered
                .into_iter()
                .find(|item| normalize_plugin_source(item.source.as_str()) == normalized_source)
            {
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
    super::chatos_skills_helpers::ensure_dir(plugins_root.as_path())?;
    super::chatos_skills_helpers::ensure_dir(git_cache_root.as_path())?;

    let repo_root = ensure_git_repo(
        repository.as_str(),
        branch.as_deref(),
        git_cache_root.as_path(),
    )
    .await?;

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
            move || {
                copy_plugin_source_from_repo(
                    repo_root.as_path(),
                    plugins_root.as_path(),
                    source.as_str(),
                )
            }
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
            let _ = skills_repo::update_plugin_install_state(user_id, plugin.source.as_str(), 0, 0)
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
