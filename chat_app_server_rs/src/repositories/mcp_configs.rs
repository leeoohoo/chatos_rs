use mongodb::bson::{doc, Bson, Document};
use serde_json::Value;

use crate::core::mongo_cursor::{collect_and_map, collect_string_field, sort_by_str_key_desc};
use crate::core::mongo_query::{filter_optional_user_id, insert_optional_user_id};
use crate::core::sql_query::{
    append_optional_user_id_filter, build_select_all_with_optional_user_id,
};
use crate::core::sql_rows::collect_string_column;
use crate::models::mcp_config::{McpConfig, McpConfigRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_doc(doc: &Document) -> Option<McpConfig> {
    Some(McpConfig {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        command: doc.get_str("command").ok()?.to_string(),
        r#type: doc.get_str("type").unwrap_or("stdio").to_string(),
        args: doc
            .get_str("args")
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(s).ok()),
        env: doc
            .get_str("env")
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(s).ok()),
        cwd: doc.get_str("cwd").ok().map(|s| s.to_string()),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        enabled: doc.get_bool("enabled").unwrap_or(true),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

pub async fn list_mcp_configs(user_id: Option<String>) -> Result<Vec<McpConfig>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let filter = filter_optional_user_id(user_id);
                let cursor = db
                    .collection::<Document>("mcp_configs")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut items: Vec<McpConfig> = collect_and_map(cursor, normalize_doc).await?;
                sort_by_str_key_desc(&mut items, |item| item.created_at.as_str());
                Ok(items)
            })
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let query =
                    build_select_all_with_optional_user_id("mcp_configs", user_id.is_some(), true);
                let mut q = sqlx::query_as::<_, McpConfigRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_config()).collect())
            })
        },
    )
    .await
}

pub async fn list_enabled_mcp_configs(user_id: Option<String>) -> Result<Vec<McpConfig>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let mut filter = doc! {};
                insert_optional_user_id(&mut filter, user_id);
                let cursor = db
                    .collection::<Document>("mcp_configs")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                collect_and_map(cursor, normalize_doc).await
            })
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let query =
                    build_select_all_with_optional_user_id("mcp_configs", user_id.is_some(), false);
                let mut q = sqlx::query_as::<_, McpConfigRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_config()).collect())
            })
        },
    )
    .await
}

pub async fn list_enabled_mcp_configs_by_ids(
    user_id: Option<String>,
    ids: &[String],
) -> Result<Vec<McpConfig>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    with_db(
        |db| {
            let user_id = user_id.clone();
            let ids = ids.to_vec();
            Box::pin(async move {
                let mut filter = doc! { "id": { "$in": ids } };
                insert_optional_user_id(&mut filter, user_id);
                let cursor = db
                    .collection::<Document>("mcp_configs")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                collect_and_map(cursor, normalize_doc).await
            })
        },
        |pool| {
            let user_id = user_id.clone();
            let ids = ids.to_vec();
            Box::pin(async move {
                let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                let mut query = format!("SELECT * FROM mcp_configs WHERE id IN ({})", placeholders);
                append_optional_user_id_filter(&mut query, user_id.is_some(), true);
                let mut q = sqlx::query_as::<_, McpConfigRow>(&query);
                for id in &ids {
                    q = q.bind(id);
                }
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_config()).collect())
            })
        },
    )
    .await
}

pub async fn get_mcp_config_by_id(id: &str) -> Result<Option<McpConfig>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("mcp_configs")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row =
                    sqlx::query_as::<_, McpConfigRow>("SELECT * FROM mcp_configs WHERE id = ?")
                        .bind(&id)
                        .fetch_optional(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_config()))
            })
        },
    )
    .await
}

pub async fn create_mcp_config(cfg: &McpConfig) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let args_str = cfg.args.as_ref().map(|v| v.to_string());
    let env_str = cfg.env.as_ref().map(|v| v.to_string());
    let cfg_mongo = cfg.clone();
    let cfg_sqlite = cfg.clone();
    let args_str_mongo = args_str.clone();
    let args_str_sqlite = args_str.clone();
    let env_str_mongo = env_str.clone();
    let env_str_sqlite = env_str.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(cfg_mongo.id.clone())),
                ("name", Bson::String(cfg_mongo.name.clone())),
                ("command", Bson::String(cfg_mongo.command.clone())),
                ("type", Bson::String(cfg_mongo.r#type.clone())),
                ("args", args_str_mongo.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("env", env_str_mongo.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("cwd", cfg_mongo.cwd.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("user_id", cfg_mongo.user_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("enabled", Bson::Boolean(cfg_mongo.enabled)),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("mcp_configs").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO mcp_configs (id, name, command, type, args, env, cwd, user_id, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&cfg_sqlite.id)
                    .bind(&cfg_sqlite.name)
                    .bind(&cfg_sqlite.command)
                    .bind(&cfg_sqlite.r#type)
                    .bind(args_str_sqlite.as_deref())
                    .bind(env_str_sqlite.as_deref())
                    .bind(&cfg_sqlite.cwd)
                    .bind(&cfg_sqlite.user_id)
                    .bind(if cfg_sqlite.enabled {1} else {0})
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

pub async fn update_mcp_config(id: &str, updates: &McpConfig) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let args_str = updates.args.as_ref().map(|v| v.to_string());
    let env_str = updates.env.as_ref().map(|v| v.to_string());
    let updates_mongo = updates.clone();
    let updates_sqlite = updates.clone();
    let args_str_mongo = args_str.clone();
    let args_str_sqlite = args_str.clone();
    let env_str_mongo = env_str.clone();
    let env_str_sqlite = env_str.clone();

    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_doc = Document::new();
                set_doc.insert("name", updates_mongo.name.clone());
                set_doc.insert("command", updates_mongo.command.clone());
                set_doc.insert("type", updates_mongo.r#type.clone());
                set_doc.insert("args", args_str_mongo.clone().map(Bson::String).unwrap_or(Bson::Null));
                set_doc.insert("env", env_str_mongo.clone().map(Bson::String).unwrap_or(Bson::Null));
                set_doc.insert("cwd", updates_mongo.cwd.clone().map(Bson::String).unwrap_or(Bson::Null));
                set_doc.insert("enabled", Bson::Boolean(updates_mongo.enabled));
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("mcp_configs").update_one(doc! { "id": id }, doc! { "$set": set_doc }, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("UPDATE mcp_configs SET name = ?, command = ?, type = ?, args = ?, env = ?, cwd = ?, enabled = ?, updated_at = ? WHERE id = ?")
                    .bind(&updates_sqlite.name)
                    .bind(&updates_sqlite.command)
                    .bind(&updates_sqlite.r#type)
                    .bind(args_str_sqlite.as_deref())
                    .bind(env_str_sqlite.as_deref())
                    .bind(&updates_sqlite.cwd)
                    .bind(if updates_sqlite.enabled {1} else {0})
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

pub async fn delete_mcp_config(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("mcp_configs")
                    .delete_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<Document>("mcp_config_applications")
                    .delete_many(doc! { "mcp_config_id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<Document>("mcp_config_profiles")
                    .delete_many(doc! { "mcp_config_id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM mcp_configs WHERE id = ?")
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

pub async fn get_app_ids_for_mcp_config(config_id: &str) -> Result<Vec<String>, String> {
    with_db(
        |db| {
            let config_id = config_id.to_string();
            Box::pin(async move {
                let cursor = db
                    .collection::<Document>("mcp_config_applications")
                    .find(doc! { "mcp_config_id": config_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                collect_string_field(cursor, "application_id").await
            })
        },
        |pool| {
            let config_id = config_id.to_string();
            Box::pin(async move {
                let rows = sqlx::query(
                    "SELECT application_id FROM mcp_config_applications WHERE mcp_config_id = ?",
                )
                .bind(&config_id)
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(collect_string_column(rows, "application_id"))
            })
        },
    )
    .await
}

pub async fn set_app_ids_for_mcp_config(config_id: &str, app_ids: &[String]) -> Result<(), String> {
    with_db(
        |db| {
            let config_id = config_id.to_string();
            let app_ids = app_ids.to_vec();
            Box::pin(async move {
                db.collection::<Document>("mcp_config_applications").delete_many(doc! { "mcp_config_id": &config_id }, None).await.map_err(|e| e.to_string())?;
                if !app_ids.is_empty() {
                    let now = chrono::Utc::now().to_rfc3339();
                    let docs: Vec<Document> = app_ids.iter().map(|aid| doc! { "id": format!("{}_{}", config_id, aid), "mcp_config_id": &config_id, "application_id": aid, "created_at": &now }).collect();
                    db.collection::<Document>("mcp_config_applications").insert_many(docs, None).await.map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        },
        |pool| {
            let config_id = config_id.to_string();
            let app_ids = app_ids.to_vec();
            Box::pin(async move {
                sqlx::query("DELETE FROM mcp_config_applications WHERE mcp_config_id = ?")
                    .bind(&config_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let now = chrono::Utc::now().to_rfc3339();
                for aid in app_ids {
                    sqlx::query("INSERT INTO mcp_config_applications (id, mcp_config_id, application_id, created_at) VALUES (?, ?, ?, ?)")
                        .bind(format!("{}_{}", config_id, aid))
                        .bind(&config_id)
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
