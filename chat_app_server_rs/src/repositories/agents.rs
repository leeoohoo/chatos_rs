use crate::core::mongo_cursor::{collect_map_sorted_desc, collect_string_field};
use crate::core::mongo_query::filter_optional_user_id;
use crate::core::sql_query::build_select_all_with_optional_user_id;
use crate::core::sql_rows::collect_string_column;
use crate::models::agent::{Agent, AgentRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};
use mongodb::bson::{doc, Bson, Document};

fn normalize_doc(doc: &Document) -> Option<Agent> {
    let mcp_ids = doc
        .get_str("mcp_config_ids")
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();
    let callable_ids = doc
        .get_str("callable_agent_ids")
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();
    Some(Agent {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        ai_model_config_id: doc.get_str("ai_model_config_id").ok()?.to_string(),
        system_context_id: doc.get_str("system_context_id").ok().map(|s| s.to_string()),
        description: doc.get_str("description").ok().map(|s| s.to_string()),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        mcp_config_ids: mcp_ids,
        callable_agent_ids: callable_ids,
        project_id: doc.get_str("project_id").ok().map(|s| s.to_string()),
        workspace_dir: doc.get_str("workspace_dir").ok().map(|s| s.to_string()),
        enabled: doc.get_bool("enabled").unwrap_or(true),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

pub async fn list_agents(user_id: Option<String>) -> Result<Vec<Agent>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let filter = filter_optional_user_id(user_id);
                let cursor = db
                    .collection::<Document>("agents")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let items: Vec<Agent> =
                    collect_map_sorted_desc(cursor, normalize_doc, |item| item.created_at.as_str())
                        .await?;
                Ok(items)
            })
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let query =
                    build_select_all_with_optional_user_id("agents", user_id.is_some(), true);
                let mut q = sqlx::query_as::<_, AgentRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_agent()).collect())
            })
        },
    )
    .await
}

pub async fn get_agent_by_id(id: &str) -> Result<Option<Agent>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("agents")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, AgentRow>("SELECT * FROM agents WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_agent()))
            })
        },
    )
    .await
}

pub async fn create_agent(data: &Agent) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let mcp_json = serde_json::to_string(&data.mcp_config_ids).unwrap_or("[]".to_string());
    let callable_json = serde_json::to_string(&data.callable_agent_ids).unwrap_or("[]".to_string());
    let data_mongo = data.clone();
    let data_sqlite = data.clone();
    let mcp_json_mongo = mcp_json.clone();
    let mcp_json_sqlite = mcp_json.clone();
    let callable_json_mongo = callable_json.clone();
    let callable_json_sqlite = callable_json.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(data_mongo.id.clone())),
                ("name", Bson::String(data_mongo.name.clone())),
                ("ai_model_config_id", Bson::String(data_mongo.ai_model_config_id.clone())),
                ("system_context_id", crate::core::values::optional_string_bson(data_mongo.system_context_id.clone())),
                ("description", crate::core::values::optional_string_bson(data_mongo.description.clone())),
                ("user_id", crate::core::values::optional_string_bson(data_mongo.user_id.clone())),
                ("mcp_config_ids", Bson::String(mcp_json_mongo.clone())),
                ("callable_agent_ids", Bson::String(callable_json_mongo.clone())),
                ("project_id", crate::core::values::optional_string_bson(data_mongo.project_id.clone())),
                ("workspace_dir", crate::core::values::optional_string_bson(data_mongo.workspace_dir.clone())),
                ("enabled", Bson::Boolean(data_mongo.enabled)),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("agents").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO agents (id, name, ai_model_config_id, system_context_id, description, user_id, mcp_config_ids, callable_agent_ids, project_id, workspace_dir, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&data_sqlite.id)
                    .bind(&data_sqlite.name)
                    .bind(&data_sqlite.ai_model_config_id)
                    .bind(&data_sqlite.system_context_id)
                    .bind(&data_sqlite.description)
                    .bind(&data_sqlite.user_id)
                    .bind(&mcp_json_sqlite)
                    .bind(&callable_json_sqlite)
                    .bind(&data_sqlite.project_id)
                    .bind(&data_sqlite.workspace_dir)
                    .bind(crate::core::values::bool_to_sqlite_int(data_sqlite.enabled))
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        }
    ).await
}

pub async fn update_agent(id: &str, updates: &Agent) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let mcp_json = serde_json::to_string(&updates.mcp_config_ids).unwrap_or("[]".to_string());
    let callable_json =
        serde_json::to_string(&updates.callable_agent_ids).unwrap_or("[]".to_string());
    let updates_mongo = updates.clone();
    let updates_sqlite = updates.clone();
    let mcp_json_mongo = mcp_json.clone();
    let mcp_json_sqlite = mcp_json.clone();
    let callable_json_mongo = callable_json.clone();
    let callable_json_sqlite = callable_json.clone();

    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_doc = Document::new();
                set_doc.insert("name", updates_mongo.name.clone());
                set_doc.insert("ai_model_config_id", updates_mongo.ai_model_config_id.clone());
                set_doc.insert("system_context_id", crate::core::values::optional_string_bson(updates_mongo.system_context_id.clone()));
                set_doc.insert("description", crate::core::values::optional_string_bson(updates_mongo.description.clone()));
                set_doc.insert("enabled", Bson::Boolean(updates_mongo.enabled));
                set_doc.insert("mcp_config_ids", Bson::String(mcp_json_mongo.clone()));
                set_doc.insert("callable_agent_ids", Bson::String(callable_json_mongo.clone()));
                set_doc.insert("project_id", crate::core::values::optional_string_bson(updates_mongo.project_id.clone()));
                set_doc.insert("workspace_dir", crate::core::values::optional_string_bson(updates_mongo.workspace_dir.clone()));
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("agents").update_one(doc! { "id": id }, doc! { "$set": set_doc }, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("UPDATE agents SET name = ?, ai_model_config_id = ?, system_context_id = ?, description = ?, enabled = ?, mcp_config_ids = ?, callable_agent_ids = ?, project_id = ?, workspace_dir = ?, updated_at = ? WHERE id = ?")
                    .bind(&updates_sqlite.name)
                    .bind(&updates_sqlite.ai_model_config_id)
                    .bind(&updates_sqlite.system_context_id)
                    .bind(&updates_sqlite.description)
                    .bind(crate::core::values::bool_to_sqlite_int(updates_sqlite.enabled))
                    .bind(&mcp_json_sqlite)
                    .bind(&callable_json_sqlite)
                    .bind(&updates_sqlite.project_id)
                    .bind(&updates_sqlite.workspace_dir)
                    .bind(&now_sqlite)
                    .bind(&id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        }
    ).await
}

pub async fn delete_agent(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("agents")
                    .delete_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<Document>("agent_applications")
                    .delete_many(doc! { "agent_id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM agents WHERE id = ?")
                    .bind(&id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn get_app_ids_for_agent(agent_id: &str) -> Result<Vec<String>, String> {
    with_db(
        |db| {
            let agent_id = agent_id.to_string();
            Box::pin(async move {
                let cursor = db
                    .collection::<Document>("agent_applications")
                    .find(doc! { "agent_id": agent_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                collect_string_field(cursor, "application_id").await
            })
        },
        |pool| {
            let agent_id = agent_id.to_string();
            Box::pin(async move {
                let rows =
                    sqlx::query("SELECT application_id FROM agent_applications WHERE agent_id = ?")
                        .bind(&agent_id)
                        .fetch_all(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                Ok(collect_string_column(rows, "application_id"))
            })
        },
    )
    .await
}

pub async fn set_app_ids_for_agent(agent_id: &str, app_ids: &[String]) -> Result<(), String> {
    with_db(
        |db| {
            let agent_id = agent_id.to_string();
            let app_ids = app_ids.to_vec();
            Box::pin(async move {
                db.collection::<Document>("agent_applications").delete_many(doc! { "agent_id": &agent_id }, None).await.map_err(|e| e.to_string())?;
                if !app_ids.is_empty() {
                    let now = crate::core::time::now_rfc3339();
                    let docs: Vec<Document> = app_ids.iter().map(|aid| doc! { "id": format!("{}_{}", agent_id, aid), "agent_id": &agent_id, "application_id": aid, "created_at": &now }).collect();
                    db.collection::<Document>("agent_applications").insert_many(docs, None).await.map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        },
        |pool| {
            let agent_id = agent_id.to_string();
            let app_ids = app_ids.to_vec();
            Box::pin(async move {
                sqlx::query("DELETE FROM agent_applications WHERE agent_id = ?").bind(&agent_id).execute(pool).await.map_err(|e| e.to_string())?;
                let now = crate::core::time::now_rfc3339();
                for aid in app_ids {
                    sqlx::query("INSERT INTO agent_applications (id, agent_id, application_id, created_at) VALUES (?, ?, ?, ?)")
                        .bind(format!("{}_{}", agent_id, aid))
                        .bind(&agent_id)
                        .bind(&aid)
                        .bind(&now)
                        .execute(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        }
    ).await
}
