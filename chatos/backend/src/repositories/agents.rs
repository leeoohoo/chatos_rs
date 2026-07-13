// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use crate::models::agent::Agent;
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

    with_db(|db| {
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
    })
    .await
}

pub async fn get_agent_by_id(agent_id: &str) -> Result<Option<Agent>, String> {
    with_db(|db| {
        let agent_id = agent_id.to_string();
        Box::pin(async move {
            db.collection::<Agent>("agents")
                .find_one(doc! { "id": agent_id }, None)
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn create_agent(agent: &Agent) -> Result<(), String> {
    with_db(|db| {
        let agent = agent.clone();
        Box::pin(async move {
            db.collection::<Agent>("agents")
                .insert_one(agent, None)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    })
    .await
}

pub async fn update_agent(agent: &Agent) -> Result<(), String> {
    with_db(|db| {
        let agent = agent.clone();
        Box::pin(async move {
            db.collection::<Agent>("agents")
                .replace_one(doc! { "id": &agent.id }, agent, None)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    })
    .await
}

pub async fn delete_agent(agent_id: &str) -> Result<bool, String> {
    with_db(|db| {
        let agent_id = agent_id.to_string();
        Box::pin(async move {
            let result = db
                .collection::<Document>("agents")
                .delete_one(doc! { "id": &agent_id }, None)
                .await
                .map_err(|e| e.to_string())?;
            Ok(result.deleted_count > 0)
        })
    })
    .await
}
