// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::{doc, Regex};
use mongodb::options::FindOptions;

use crate::models::memory_skill::{MemorySkill, MemorySkillPlugin};

use super::db::with_db;

pub async fn list_skills(
    user_ids: &[String],
    plugin_source: Option<&str>,
    query: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<MemorySkill>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    with_db(|db| {
        let user_ids = user_ids.to_vec();
        let plugin_source = plugin_source.map(|value| value.to_string());
        let query = query.map(|value| value.to_string());
        Box::pin(async move {
            let mut filter = if user_ids.len() == 1 {
                doc! { "user_id": user_ids[0].clone() }
            } else {
                doc! { "user_id": { "$in": user_ids } }
            };
            if let Some(value) = plugin_source
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                filter.insert("plugin_source", value);
            }
            if let Some(value) = query
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                let regex = Regex {
                    pattern: value.to_string(),
                    options: "i".to_string(),
                };
                filter.insert(
                    "$or",
                    vec![
                        doc! { "name": { "$regex": regex.clone() } },
                        doc! { "description": { "$regex": regex.clone() } },
                        doc! { "source_path": { "$regex": regex } },
                    ],
                );
            }

            let options = FindOptions::builder()
                .sort(doc! { "updated_at": -1 })
                .limit(Some(limit.clamp(1, 500)))
                .skip(Some(offset.max(0) as u64))
                .build();

            let cursor = db
                .collection::<MemorySkill>("memory_skills")
                .find(filter, options)
                .await
                .map_err(|e| e.to_string())?;
            cursor
                .try_collect::<Vec<MemorySkill>>()
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn get_skill_by_id(
    user_ids: &[String],
    skill_id: &str,
) -> Result<Option<MemorySkill>, String> {
    if user_ids.is_empty() {
        return Ok(None);
    }

    with_db(|db| {
        let user_ids = user_ids.to_vec();
        let skill_id = skill_id.to_string();
        Box::pin(async move {
            let filter = if user_ids.len() == 1 {
                doc! { "id": &skill_id, "user_id": user_ids[0].clone() }
            } else {
                doc! { "id": &skill_id, "user_id": { "$in": user_ids } }
            };
            db.collection::<MemorySkill>("memory_skills")
                .find_one(filter, None)
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn list_plugins_by_user_ids(
    user_ids: &[String],
    limit: i64,
    offset: i64,
) -> Result<Vec<MemorySkillPlugin>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    with_db(|db| {
        let user_ids = user_ids.to_vec();
        Box::pin(async move {
            let filter = if user_ids.len() == 1 {
                doc! { "user_id": user_ids[0].clone() }
            } else {
                doc! { "user_id": { "$in": user_ids } }
            };
            let options = FindOptions::builder()
                .sort(doc! { "updated_at": -1 })
                .limit(Some(limit.clamp(1, 1000)))
                .skip(Some(offset.max(0) as u64))
                .build();
            let cursor = db
                .collection::<MemorySkillPlugin>("memory_skill_plugins")
                .find(filter, options)
                .await
                .map_err(|e| e.to_string())?;
            cursor
                .try_collect::<Vec<MemorySkillPlugin>>()
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn get_plugins_by_sources_for_user_ids(
    user_ids: &[String],
    sources: &[String],
) -> Result<Vec<MemorySkillPlugin>, String> {
    if user_ids.is_empty() || sources.is_empty() {
        return Ok(Vec::new());
    }

    with_db(|db| {
        let user_ids = user_ids.to_vec();
        let sources = sources.to_vec();
        Box::pin(async move {
            let filter = if user_ids.len() == 1 {
                doc! { "user_id": user_ids[0].clone(), "source": { "$in": sources } }
            } else {
                doc! { "user_id": { "$in": user_ids }, "source": { "$in": sources } }
            };
            let cursor = db
                .collection::<MemorySkillPlugin>("memory_skill_plugins")
                .find(filter, None)
                .await
                .map_err(|e| e.to_string())?;
            cursor
                .try_collect::<Vec<MemorySkillPlugin>>()
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn get_plugin_by_source_for_user_ids(
    user_ids: &[String],
    source: &str,
) -> Result<Option<MemorySkillPlugin>, String> {
    let items = get_plugins_by_sources_for_user_ids(user_ids, &[source.to_string()]).await?;
    if items.is_empty() {
        return Ok(None);
    }
    for user_id in user_ids {
        if let Some(item) = items.iter().find(|item| item.user_id == *user_id) {
            return Ok(Some(item.clone()));
        }
    }
    Ok(items.first().cloned())
}

pub async fn get_plugins_by_sources(
    user_id: &str,
    sources: &[String],
) -> Result<Vec<MemorySkillPlugin>, String> {
    get_plugins_by_sources_for_user_ids(&[user_id.to_string()], sources).await
}

pub async fn upsert_plugin(mut plugin: MemorySkillPlugin) -> Result<MemorySkillPlugin, String> {
    plugin.updated_at = crate::core::time::now_rfc3339();
    with_db(|db| {
        let plugin = plugin.clone();
        Box::pin(async move {
            let filter = doc! { "id": &plugin.id };
            let exists = db
                .collection::<MemorySkillPlugin>("memory_skill_plugins")
                .find_one(filter.clone(), None)
                .await
                .map_err(|e| e.to_string())?
                .is_some();
            if exists {
                db.collection::<MemorySkillPlugin>("memory_skill_plugins")
                    .replace_one(filter, plugin.clone(), None)
                    .await
                    .map_err(|e| e.to_string())?;
            } else {
                db.collection::<MemorySkillPlugin>("memory_skill_plugins")
                    .insert_one(plugin.clone(), None)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            Ok(plugin)
        })
    })
    .await
}

pub async fn replace_skills_for_plugin(
    user_id: &str,
    plugin_source: &str,
    skills: Vec<MemorySkill>,
) -> Result<usize, String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        let plugin_source = plugin_source.to_string();
        let skills = skills.clone();
        Box::pin(async move {
            db.collection::<MemorySkill>("memory_skills")
                .delete_many(
                    doc! { "user_id": &user_id, "plugin_source": &plugin_source },
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
            if skills.is_empty() {
                return Ok(0usize);
            }
            db.collection::<MemorySkill>("memory_skills")
                .insert_many(skills.clone(), None)
                .await
                .map_err(|e| e.to_string())?;
            Ok(skills.len())
        })
    })
    .await
}

pub async fn update_plugin_install_state(
    user_id: &str,
    source: &str,
    installed_skill_count: i64,
    discoverable_skills: i64,
) -> Result<Option<MemorySkillPlugin>, String> {
    let existing = get_plugin_by_source_for_user_ids(&[user_id.to_string()], source).await?;
    let Some(mut plugin) = existing else {
        return Ok(None);
    };

    plugin.installed = true;
    plugin.installed_skill_count = installed_skill_count.max(0);
    plugin.discoverable_skills = discoverable_skills.max(installed_skill_count).max(0);
    plugin.updated_at = crate::core::time::now_rfc3339();

    upsert_plugin(plugin.clone()).await?;
    Ok(Some(plugin))
}
