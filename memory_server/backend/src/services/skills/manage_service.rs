use serde_json::{json, Value};

use crate::models::MemorySkillPlugin;
use crate::repositories::skills as skills_repo;
use crate::state::AppState;

use super::io::{
    build_skills_from_plugin_async, copy_plugin_source_from_repo_async,
    discover_skill_entries_async, ensure_dir_async, ensure_git_repo_async,
    load_plugin_candidates_from_repo_async, normalize_plugin_source,
    resolve_plugin_root_from_cache, resolve_skill_state_root, unique_strings,
};
use super::io_helpers::hash_id;

pub(crate) struct ImportSkillsOutcome {
    pub(crate) repository: String,
    pub(crate) branch: Option<String>,
    pub(crate) imported_sources: Vec<String>,
    pub(crate) details: Vec<Value>,
}

pub(crate) async fn import_skills_from_git(
    state: &AppState,
    scope_user_id: &str,
    repository: String,
    branch: Option<String>,
    marketplace_path: Option<String>,
    plugins_path: Option<String>,
) -> Result<ImportSkillsOutcome, String> {
    let state_root = resolve_skill_state_root(scope_user_id);
    let plugins_root = state_root.join("plugins");
    let git_cache_root = state_root.join("git-cache");

    ensure_dir_async(plugins_root.clone())
        .await
        .map_err(|err| format!("prepare plugin cache failed: {}", err))?;
    ensure_dir_async(git_cache_root.clone())
        .await
        .map_err(|err| format!("prepare git cache failed: {}", err))?;

    let repo_root =
        ensure_git_repo_async(repository.clone(), branch.clone(), git_cache_root.clone())
            .await
            .map_err(|err| format!("git import failed: {}", err))?;

    let candidates =
        load_plugin_candidates_from_repo_async(repo_root.clone(), marketplace_path, plugins_path)
            .await
            .map_err(|err| format!("parse plugin definitions failed: {}", err))?;
    if candidates.is_empty() {
        return Err("no plugins discovered from repository".to_string());
    }

    let sources = candidates
        .iter()
        .map(|item| item.source.clone())
        .collect::<Vec<_>>();
    let existing = skills_repo::get_plugins_by_sources(&state.pool, scope_user_id, &sources)
        .await
        .unwrap_or_default();
    let existing_by_source = existing
        .into_iter()
        .map(|item| (item.source.clone(), item))
        .collect::<std::collections::HashMap<_, _>>();

    let mut imported_sources = Vec::new();
    let mut details = Vec::new();
    for candidate in candidates {
        let cache_rel = match copy_plugin_source_from_repo_async(
            repo_root.clone(),
            plugins_root.clone(),
            candidate.source.clone(),
        )
        .await
        {
            Ok(value) => value,
            Err(err) => {
                details.push(json!({
                    "source": candidate.source,
                    "ok": false,
                    "error": err
                }));
                continue;
            }
        };

        let plugin_root = plugins_root.join(cache_rel.as_str());
        let discoverable_skills = match discover_skill_entries_async(plugin_root.clone()).await {
            Ok(entries) => entries.len().min(i64::MAX as usize) as i64,
            Err(err) => {
                details.push(json!({
                    "source": candidate.source,
                    "ok": false,
                    "error": err
                }));
                continue;
            }
        };
        let previous = existing_by_source.get(candidate.source.as_str());
        let plugin = MemorySkillPlugin {
            id: previous
                .map(|item| item.id.clone())
                .unwrap_or_else(|| hash_id(&["plugin", scope_user_id, candidate.source.as_str()])),
            user_id: scope_user_id.to_string(),
            source: candidate.source.clone(),
            name: candidate.name.clone(),
            category: candidate.category.clone(),
            description: candidate.description.clone(),
            version: candidate.version.clone(),
            repository: Some(repository.clone()),
            branch: branch.clone(),
            cache_path: Some(cache_rel.clone()),
            installed: previous.map(|item| item.installed).unwrap_or(false),
            discoverable_skills,
            installed_skill_count: previous.map(|item| item.installed_skill_count).unwrap_or(0),
            updated_at: crate::repositories::now_rfc3339(),
        };

        match skills_repo::upsert_plugin(&state.pool, plugin).await {
            Ok(saved) => {
                imported_sources.push(saved.source.clone());
                details.push(json!({
                    "source": saved.source,
                    "name": saved.name,
                    "discoverable_skills": saved.discoverable_skills,
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

pub(crate) async fn list_all_plugin_sources(
    state: &AppState,
    user_id: &str,
) -> Result<Vec<String>, String> {
    let items = skills_repo::list_plugins(&state.pool, user_id, 500, 0).await?;
    Ok(items
        .into_iter()
        .map(|item| item.source)
        .collect::<Vec<_>>())
}

pub(crate) async fn install_skill_plugins(
    state: &AppState,
    user_id: &str,
    sources: &[String],
) -> Result<Value, String> {
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

    let plugins =
        skills_repo::get_plugins_by_sources(&state.pool, user_id, &normalized_sources).await?;
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

        let (skills, discoverable_count) = build_skills_from_plugin_async(
            plugin_root.clone(),
            user_id.to_string(),
            plugin.source.clone(),
            plugin.version.clone(),
        )
        .await?;
        if discoverable_count <= 0 {
            let _ = skills_repo::replace_skills_for_plugin(
                &state.pool,
                user_id,
                plugin.source.as_str(),
                Vec::new(),
            )
            .await;
            let _ = skills_repo::update_plugin_install_state(
                &state.pool,
                user_id,
                plugin.source.as_str(),
                0,
                0,
            )
            .await;
            skipped += 1;
            details.push(json!({
                "source": plugin.source,
                "ok": false,
                "reason": "no skills discovered in plugin"
            }));
            continue;
        }

        let installed_count = skills_repo::replace_skills_for_plugin(
            &state.pool,
            user_id,
            plugin.source.as_str(),
            skills,
        )
        .await?;
        let _ = skills_repo::update_plugin_install_state(
            &state.pool,
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
