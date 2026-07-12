// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use crate::models::agent::{Agent, AgentRow};
use crate::repositories::db::with_db;

pub async fn list_agents_by_user_ids(
    user_ids: &[String],
    enabled: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Agent>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    with_db(
        |db| {
            let user_ids = user_ids.to_vec();
            Box::pin(async move {
                let mut filter = if user_ids.len() == 1 {
                    doc! { "user_id": user_ids[0].clone() }
                } else {
                    doc! { "user_id": { "$in": user_ids } }
                };
                if let Some(value) = enabled {
                    filter.insert("enabled", value);
                }
                let options = FindOptions::builder()
                    .sort(doc! { "updated_at": -1, "created_at": -1 })
                    .limit(Some(limit.clamp(1, 500)))
                    .skip(Some(offset.max(0) as u64))
                    .build();
                let cursor = db
                    .collection::<Agent>("agents")
                    .find(filter, options)
                    .await
                    .map_err(|e| e.to_string())?;
                cursor
                    .try_collect::<Vec<Agent>>()
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let user_ids = user_ids.to_vec();
            Box::pin(async move {
                let mut sql = "SELECT * FROM agents WHERE user_id IN (".to_string();
                sql.push_str(&vec!["?"; user_ids.len()].join(","));
                sql.push(')');
                if enabled.is_some() {
                    sql.push_str(" AND enabled = ?");
                }
                sql.push_str(" ORDER BY updated_at DESC, created_at DESC LIMIT ? OFFSET ?");

                let mut query = sqlx::query_as::<_, AgentRow>(sqlx::AssertSqlSafe(sql));
                for user_id in &user_ids {
                    query = query.bind(user_id);
                }
                if let Some(value) = enabled {
                    query = query.bind(crate::core::values::bool_to_sqlite_int(value));
                }
                query = query.bind(limit.clamp(1, 500)).bind(offset.max(0));
                let rows = query.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(AgentRow::into_agent).collect())
            })
        },
    )
    .await
}

pub async fn get_agent_by_id(agent_id: &str) -> Result<Option<Agent>, String> {
    with_db(
        |db| {
            let agent_id = agent_id.to_string();
            Box::pin(async move {
                db.collection::<Agent>("agents")
                    .find_one(doc! { "id": agent_id }, None)
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let agent_id = agent_id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, AgentRow>("SELECT * FROM agents WHERE id = ?")
                    .bind(&agent_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.map(AgentRow::into_agent))
            })
        },
    )
    .await
}

pub async fn create_agent(agent: &Agent) -> Result<(), String> {
    with_db(
        |db| {
            let agent = agent.clone();
            Box::pin(async move {
                db.collection::<Agent>("agents")
                    .insert_one(agent, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let agent = agent.clone();
            Box::pin(async move {
                let plugin_sources =
                    serde_json::to_string(&agent.plugin_sources).map_err(|e| e.to_string())?;
                let skills = serde_json::to_string(&agent.skills).map_err(|e| e.to_string())?;
                let skill_ids =
                    serde_json::to_string(&agent.skill_ids).map_err(|e| e.to_string())?;
                let default_skill_ids = serde_json::to_string(&agent.default_skill_ids)
                    .map_err(|e| e.to_string())?;
                let mcp_policy = agent.mcp_policy.as_ref().map(ValueToString::to_string_value);
                let project_policy = agent
                    .project_policy
                    .as_ref()
                    .map(ValueToString::to_string_value);

                sqlx::query(
                    "INSERT INTO agents \
                    (id, user_id, name, description, category, role_definition, task_runner_agent_account_id, plugin_sources, skills, skill_ids, default_skill_ids, mcp_policy, project_policy, enabled, created_at, updated_at) \
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&agent.id)
                .bind(&agent.user_id)
                .bind(&agent.name)
                .bind(&agent.description)
                .bind(&agent.category)
                .bind(&agent.role_definition)
                .bind(&agent.task_runner_agent_account_id)
                .bind(&plugin_sources)
                .bind(&skills)
                .bind(&skill_ids)
                .bind(&default_skill_ids)
                .bind(mcp_policy.as_deref())
                .bind(project_policy.as_deref())
                .bind(crate::core::values::bool_to_sqlite_int(agent.enabled))
                .bind(&agent.created_at)
                .bind(&agent.updated_at)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn update_agent(agent: &Agent) -> Result<(), String> {
    with_db(
        |db| {
            let agent = agent.clone();
            Box::pin(async move {
                db.collection::<Agent>("agents")
                    .replace_one(doc! { "id": &agent.id }, agent, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let agent = agent.clone();
            Box::pin(async move {
                let plugin_sources =
                    serde_json::to_string(&agent.plugin_sources).map_err(|e| e.to_string())?;
                let skills = serde_json::to_string(&agent.skills).map_err(|e| e.to_string())?;
                let skill_ids =
                    serde_json::to_string(&agent.skill_ids).map_err(|e| e.to_string())?;
                let default_skill_ids =
                    serde_json::to_string(&agent.default_skill_ids).map_err(|e| e.to_string())?;
                let mcp_policy = agent
                    .mcp_policy
                    .as_ref()
                    .map(ValueToString::to_string_value);
                let project_policy = agent
                    .project_policy
                    .as_ref()
                    .map(ValueToString::to_string_value);

                sqlx::query(
                    "UPDATE agents SET \
                    user_id = ?, \
                    name = ?, \
                    description = ?, \
                    category = ?, \
                    role_definition = ?, \
                    task_runner_agent_account_id = ?, \
                    plugin_sources = ?, \
                    skills = ?, \
                    skill_ids = ?, \
                    default_skill_ids = ?, \
                    mcp_policy = ?, \
                    project_policy = ?, \
                    enabled = ?, \
                    updated_at = ? \
                    WHERE id = ?",
                )
                .bind(&agent.user_id)
                .bind(&agent.name)
                .bind(&agent.description)
                .bind(&agent.category)
                .bind(&agent.role_definition)
                .bind(&agent.task_runner_agent_account_id)
                .bind(&plugin_sources)
                .bind(&skills)
                .bind(&skill_ids)
                .bind(&default_skill_ids)
                .bind(mcp_policy.as_deref())
                .bind(project_policy.as_deref())
                .bind(crate::core::values::bool_to_sqlite_int(agent.enabled))
                .bind(&agent.updated_at)
                .bind(&agent.id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn delete_agent(agent_id: &str) -> Result<bool, String> {
    with_db(
        |db| {
            let agent_id = agent_id.to_string();
            Box::pin(async move {
                let result = db
                    .collection::<Document>("agents")
                    .delete_one(doc! { "id": &agent_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(result.deleted_count > 0)
            })
        },
        |pool| {
            let agent_id = agent_id.to_string();
            Box::pin(async move {
                let result = sqlx::query("DELETE FROM agents WHERE id = ?")
                    .bind(&agent_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(result.rows_affected() > 0)
            })
        },
    )
    .await
}

trait ValueToString {
    fn to_string_value(&self) -> String;
}

impl ValueToString for serde_json::Value {
    fn to_string_value(&self) -> String {
        self.to_string()
    }
}
