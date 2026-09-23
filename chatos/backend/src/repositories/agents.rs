// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::agent::Agent;
use crate::repositories::db::{db_error, decode_all, decode_optional, json, timestamp, with_db};

pub async fn list_agents_by_user_ids(
    user_ids: &[String],
    enabled: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Agent>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    with_db(|pool| Box::pin(async move {
        decode_all(sqlx::query_scalar("SELECT data FROM agents WHERE user_id=ANY($1) AND ($2::bool IS NULL OR enabled=$2) ORDER BY updated_at DESC,created_at DESC LIMIT $3 OFFSET $4")
            .bind(user_ids).bind(enabled).bind(limit.clamp(1,500)).bind(offset.max(0)).fetch_all(pool).await.map_err(db_error)?)
    })).await
}

pub async fn get_agent_by_id(agent_id: &str) -> Result<Option<Agent>, String> {
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar("SELECT data FROM agents WHERE id=$1")
                    .bind(agent_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(db_error)?,
            )
        })
    })
    .await
}

pub async fn create_agent(agent: &Agent) -> Result<(), String> {
    with_db(|pool| Box::pin(async move {
        sqlx::query("INSERT INTO agents(id,user_id,enabled,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(&agent.id).bind(&agent.user_id).bind(agent.enabled).bind(timestamp(&agent.created_at)?).bind(timestamp(&agent.updated_at)?).bind(json(agent)?)
            .execute(pool).await.map(|_|()).map_err(db_error)
    })).await
}

pub async fn update_agent(agent: &Agent) -> Result<(), String> {
    with_db(|pool| Box::pin(async move {
        sqlx::query("UPDATE agents SET user_id=$1,enabled=$2,created_at=$3,updated_at=$4,data=$5 WHERE id=$6")
            .bind(&agent.user_id).bind(agent.enabled).bind(timestamp(&agent.created_at)?).bind(timestamp(&agent.updated_at)?).bind(json(agent)?).bind(&agent.id)
            .execute(pool).await.map(|_|()).map_err(db_error)
    })).await
}

pub async fn delete_agent(agent_id: &str) -> Result<bool, String> {
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM agents WHERE id=$1")
                .bind(agent_id)
                .execute(pool)
                .await
                .map(|result| result.rows_affected() > 0)
                .map_err(db_error)
        })
    })
    .await
}
