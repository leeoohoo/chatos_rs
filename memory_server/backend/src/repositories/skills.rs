use futures_util::TryStreamExt;
use mongodb::bson::{doc, Regex};
use mongodb::options::FindOptions;

use crate::db::Db;
use crate::models::{MemorySkill, MemorySkillPlugin};

use super::now_rfc3339;

fn skill_collection(db: &Db) -> mongodb::Collection<MemorySkill> {
    db.collection::<MemorySkill>("memory_skills")
}

fn plugin_collection(db: &Db) -> mongodb::Collection<MemorySkillPlugin> {
    db.collection::<MemorySkillPlugin>("memory_skill_plugins")
}

pub async fn list_skills(
    db: &Db,
    user_ids: &[String],
    plugin_source: Option<&str>,
    query: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<MemorySkill>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut filter = if user_ids.len() == 1 {
        doc! { "user_id": user_ids[0].clone() }
    } else {
        doc! { "user_id": { "$in": user_ids } }
    };
    if let Some(value) = plugin_source
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        filter.insert("plugin_source", value);
    }

    if let Some(value) = query.map(str::trim).filter(|value| !value.is_empty()) {
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
        .sort(doc! {"updated_at": -1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = skill_collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn get_skill_by_id(
    db: &Db,
    user_ids: &[String],
    skill_id: &str,
) -> Result<Option<MemorySkill>, String> {
    if user_ids.is_empty() {
        return Ok(None);
    }
    let filter = if user_ids.len() == 1 {
        doc! { "id": skill_id, "user_id": user_ids[0].clone() }
    } else {
        doc! { "id": skill_id, "user_id": { "$in": user_ids } }
    };

    skill_collection(db)
        .find_one(filter)
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_skills_by_ids(
    db: &Db,
    user_ids: &[String],
    skill_ids: &[String],
) -> Result<Vec<MemorySkill>, String> {
    if user_ids.is_empty() || skill_ids.is_empty() {
        return Ok(Vec::new());
    }

    let filter = if user_ids.len() == 1 {
        doc! { "user_id": user_ids[0].clone(), "id": { "$in": skill_ids } }
    } else {
        doc! { "user_id": { "$in": user_ids }, "id": { "$in": skill_ids } }
    };

    let cursor = skill_collection(db)
        .find(filter)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

#[allow(dead_code)]
pub async fn list_skills_by_plugin_sources_for_user_ids(
    db: &Db,
    user_ids: &[String],
    plugin_sources: &[String],
) -> Result<Vec<MemorySkill>, String> {
    if user_ids.is_empty() || plugin_sources.is_empty() {
        return Ok(Vec::new());
    }

    let filter = if user_ids.len() == 1 {
        doc! { "user_id": user_ids[0].clone(), "plugin_source": { "$in": plugin_sources } }
    } else {
        doc! { "user_id": { "$in": user_ids }, "plugin_source": { "$in": plugin_sources } }
    };

    let options = FindOptions::builder()
        .sort(doc! {"plugin_source": 1, "name": 1, "source_path": 1})
        .build();

    let cursor = skill_collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_plugins(
    db: &Db,
    user_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<MemorySkillPlugin>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .projection(doc! { "content": 0, "commands": 0 })
        .build();

    let cursor = plugin_collection(db)
        .find(doc! { "user_id": user_id })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_plugins_by_user_ids(
    db: &Db,
    user_ids: &[String],
    limit: i64,
    offset: i64,
) -> Result<Vec<MemorySkillPlugin>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    let filter = if user_ids.len() == 1 {
        doc! { "user_id": user_ids[0].clone() }
    } else {
        doc! { "user_id": { "$in": user_ids } }
    };

    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1})
        .limit(Some(limit.max(1).min(1000)))
        .skip(Some(offset.max(0) as u64))
        .projection(doc! { "content": 0, "commands": 0 })
        .build();

    let cursor = plugin_collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn get_plugins_by_sources(
    db: &Db,
    user_id: &str,
    sources: &[String],
) -> Result<Vec<MemorySkillPlugin>, String> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }

    let cursor = plugin_collection(db)
        .find(doc! { "user_id": user_id, "source": { "$in": sources } })
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn get_plugins_by_sources_for_user_ids(
    db: &Db,
    user_ids: &[String],
    sources: &[String],
) -> Result<Vec<MemorySkillPlugin>, String> {
    if user_ids.is_empty() || sources.is_empty() {
        return Ok(Vec::new());
    }

    let filter = if user_ids.len() == 1 {
        doc! { "user_id": user_ids[0].clone(), "source": { "$in": sources } }
    } else {
        doc! { "user_id": { "$in": user_ids }, "source": { "$in": sources } }
    };

    let cursor = plugin_collection(db)
        .find(filter)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn get_plugin_by_source_for_user_ids(
    db: &Db,
    user_ids: &[String],
    source: &str,
) -> Result<Option<MemorySkillPlugin>, String> {
    if user_ids.is_empty() {
        return Ok(None);
    }
    let filter = if user_ids.len() == 1 {
        doc! { "user_id": user_ids[0].clone(), "source": source }
    } else {
        doc! { "user_id": { "$in": user_ids }, "source": source }
    };
    let options = FindOptions::builder().sort(doc! {"updated_at": -1}).build();
    let cursor = plugin_collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    let items = cursor
        .try_collect::<Vec<MemorySkillPlugin>>()
        .await
        .map_err(|e| e.to_string())?;
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

pub async fn upsert_plugin(
    db: &Db,
    mut plugin: MemorySkillPlugin,
) -> Result<MemorySkillPlugin, String> {
    plugin.updated_at = now_rfc3339();
    let filter = doc! { "id": &plugin.id };
    let exists = plugin_collection(db)
        .find_one(filter.clone())
        .await
        .map_err(|e| e.to_string())?
        .is_some();

    if exists {
        plugin_collection(db)
            .replace_one(filter, plugin.clone())
            .await
            .map_err(|e| e.to_string())?;
    } else {
        plugin_collection(db)
            .insert_one(plugin.clone())
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(plugin)
}

pub async fn replace_skills_for_plugin(
    db: &Db,
    user_id: &str,
    plugin_source: &str,
    skills: Vec<MemorySkill>,
) -> Result<usize, String> {
    skill_collection(db)
        .delete_many(doc! { "user_id": user_id, "plugin_source": plugin_source })
        .await
        .map_err(|e| e.to_string())?;

    if skills.is_empty() {
        return Ok(0);
    }

    skill_collection(db)
        .insert_many(skills.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(skills.len())
}

pub async fn update_plugin_install_state(
    db: &Db,
    user_id: &str,
    source: &str,
    installed_skill_count: i64,
    discoverable_skills: i64,
) -> Result<Option<MemorySkillPlugin>, String> {
    let existing = plugin_collection(db)
        .find_one(doc! { "user_id": user_id, "source": source })
        .await
        .map_err(|e| e.to_string())?;
    let Some(mut plugin) = existing else {
        return Ok(None);
    };

    plugin.installed = true;
    plugin.installed_skill_count = installed_skill_count.max(0);
    plugin.discoverable_skills = discoverable_skills.max(installed_skill_count).max(0);
    plugin.updated_at = now_rfc3339();

    plugin_collection(db)
        .replace_one(doc! { "id": &plugin.id }, plugin.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(plugin))
}
