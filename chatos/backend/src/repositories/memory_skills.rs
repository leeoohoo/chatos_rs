// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#![allow(
    dead_code,
    reason = "read-only queries retained for one-time legacy ChatOS Skill migration"
)]

use crate::models::memory_skill::{MemorySkill, MemorySkillPlugin};
use crate::repositories::db::{db_error, decode_all, decode_optional, with_db};

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
    let plugin_source = plugin_source.map(str::trim).filter(|v| !v.is_empty());
    let search = query
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| format!("%{v}%"));
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM memory_skills WHERE user_id=ANY($1) AND ($2::text IS NULL OR plugin_source=$2) AND ($3::text IS NULL OR name ILIKE $3 OR data->>'description' ILIKE $3 OR data->>'source_path' ILIKE $3) ORDER BY updated_at DESC LIMIT $4 OFFSET $5").bind(user_ids).bind(plugin_source).bind(search).bind(limit.clamp(1,500)).bind(offset.max(0)).fetch_all(pool).await.map_err(db_error)?)})).await
}
pub async fn get_skill_by_id(
    user_ids: &[String],
    skill_id: &str,
) -> Result<Option<MemorySkill>, String> {
    if user_ids.is_empty() {
        return Ok(None);
    }
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar(
                    "SELECT data FROM memory_skills WHERE id=$1 AND user_id=ANY($2)",
                )
                .bind(skill_id)
                .bind(user_ids)
                .fetch_optional(pool)
                .await
                .map_err(db_error)?,
            )
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
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM memory_skill_plugins WHERE user_id=ANY($1) ORDER BY updated_at DESC LIMIT $2 OFFSET $3").bind(user_ids).bind(limit.clamp(1,1000)).bind(offset.max(0)).fetch_all(pool).await.map_err(db_error)?)})).await
}
pub async fn get_plugins_by_sources_for_user_ids(
    user_ids: &[String],
    sources: &[String],
) -> Result<Vec<MemorySkillPlugin>, String> {
    if user_ids.is_empty() || sources.is_empty() {
        return Ok(Vec::new());
    }
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM memory_skill_plugins WHERE user_id=ANY($1) AND source=ANY($2)").bind(user_ids).bind(sources).fetch_all(pool).await.map_err(db_error)?)})).await
}
pub async fn get_plugin_by_id_for_user_ids(
    user_ids: &[String],
    plugin_id: &str,
) -> Result<Option<MemorySkillPlugin>, String> {
    if user_ids.is_empty() {
        return Ok(None);
    }
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar(
                    "SELECT data FROM memory_skill_plugins WHERE id=$1 AND user_id=ANY($2)",
                )
                .bind(plugin_id)
                .bind(user_ids)
                .fetch_optional(pool)
                .await
                .map_err(db_error)?,
            )
        })
    })
    .await
}
