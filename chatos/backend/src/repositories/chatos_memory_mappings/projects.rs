// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::{normalize_optional_text, normalize_project_id};
use crate::models::memory_mapping::ChatosMemoryProject;
use crate::repositories::db::{db_error, decode_all, decode_optional, json, timestamp, with_db};

#[derive(Debug, Clone)]
pub struct UpsertMemoryProjectInput {
    pub user_id: String,
    pub project_id: String,
    pub name: String,
    pub root_path: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub is_virtual: Option<i64>,
}
pub async fn get_project_by_user_and_project_id(
    user_id: &str,
    project_id: &str,
) -> Result<Option<ChatosMemoryProject>, String> {
    let project_id = normalize_project_id(project_id);
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar(
                    "SELECT data FROM chatos_memory_projects WHERE user_id=$1 AND project_id=$2",
                )
                .bind(user_id)
                .bind(project_id)
                .fetch_optional(pool)
                .await
                .map_err(db_error)?,
            )
        })
    })
    .await
}
pub async fn upsert_memory_project(
    input: UpsertMemoryProjectInput,
) -> Result<Option<ChatosMemoryProject>, String> {
    let now = crate::core::time::now_rfc3339();
    let project_id = normalize_project_id(&input.project_id);
    let status =
        normalize_optional_text(input.status.as_deref()).unwrap_or_else(|| "active".to_string());
    let mut record = get_project_by_user_and_project_id(&input.user_id, &project_id)
        .await?
        .unwrap_or(ChatosMemoryProject {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: input.user_id.clone(),
            project_id: project_id.clone(),
            name: input.name.clone(),
            root_path: None,
            description: None,
            status: status.clone(),
            is_virtual: 0,
            created_at: now.clone(),
            updated_at: now.clone(),
            archived_at: None,
        });
    record.name = input.name;
    record.root_path = input.root_path;
    record.description = input.description;
    record.status = status;
    record.is_virtual = input.is_virtual.unwrap_or(0).max(0);
    record.updated_at = now.clone();
    record.archived_at =
        ((record.status == "archived") || (record.status == "deleted")).then_some(now);
    with_db(|pool|Box::pin(async move{let value=sqlx::query_scalar::<_,sqlx::types::Json<serde_json::Value>>("INSERT INTO chatos_memory_projects(id,user_id,project_id,status,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(user_id,project_id) DO UPDATE SET status=EXCLUDED.status,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data RETURNING data").bind(&record.id).bind(&record.user_id).bind(&record.project_id).bind(&record.status).bind(timestamp(&record.created_at)?).bind(timestamp(&record.updated_at)?).bind(json(&record)?).fetch_one(pool).await.map_err(db_error)?;Ok(Some(serde_json::from_value(value.0).map_err(|e|e.to_string())?))})).await
}
pub async fn list_projects_by_ids(
    user_id: &str,
    ids: &[String],
) -> Result<Vec<ChatosMemoryProject>, String> {
    let ids = ids
        .iter()
        .filter_map(|v| normalize_optional_text(Some(v)))
        .map(|v| normalize_project_id(&v))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM chatos_memory_projects WHERE user_id=$1 AND project_id=ANY($2)").bind(user_id).bind(ids).fetch_all(pool).await.map_err(db_error)?)})).await
}
pub async fn list_memory_projects(
    user_id: &str,
    status: Option<&str>,
    include_virtual: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosMemoryProject>, String> {
    let status = normalize_optional_text(status);
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM chatos_memory_projects WHERE user_id=$1 AND ($2::text IS NULL OR status=$2) AND ($3 OR COALESCE((data->>'is_virtual')::bigint,0)=0) ORDER BY updated_at DESC,created_at DESC LIMIT $4 OFFSET $5").bind(user_id).bind(status).bind(include_virtual).bind(limit.clamp(1,500)).bind(offset.max(0)).fetch_all(pool).await.map_err(db_error)?)})).await
}
