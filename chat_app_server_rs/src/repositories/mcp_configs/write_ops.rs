use mongodb::bson::{doc, Bson, Document};

use crate::models::mcp_config::McpConfig;
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

pub async fn create_mcp_config(cfg: &McpConfig) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
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
                (
                    "args",
                    crate::core::values::optional_string_bson(args_str_mongo.clone()),
                ),
                (
                    "env",
                    crate::core::values::optional_string_bson(env_str_mongo.clone()),
                ),
                (
                    "cwd",
                    crate::core::values::optional_string_bson(cfg_mongo.cwd.clone()),
                ),
                (
                    "user_id",
                    crate::core::values::optional_string_bson(cfg_mongo.user_id.clone()),
                ),
                ("enabled", Bson::Boolean(cfg_mongo.enabled)),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("mcp_configs")
                    .insert_one(doc, None)
                    .await
                    .map_err(|e| e.to_string())?;
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
                    .bind(crate::core::values::bool_to_sqlite_int(cfg_sqlite.enabled))
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn update_mcp_config(id: &str, updates: &McpConfig) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
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
                set_doc.insert(
                    "args",
                    crate::core::values::optional_string_bson(args_str_mongo.clone()),
                );
                set_doc.insert(
                    "env",
                    crate::core::values::optional_string_bson(env_str_mongo.clone()),
                );
                set_doc.insert(
                    "cwd",
                    crate::core::values::optional_string_bson(updates_mongo.cwd.clone()),
                );
                set_doc.insert("enabled", Bson::Boolean(updates_mongo.enabled));
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("mcp_configs")
                    .update_one(doc! { "id": id }, doc! { "$set": set_doc }, None)
                    .await
                    .map_err(|e| e.to_string())?;
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
                    .bind(crate::core::values::bool_to_sqlite_int(updates_sqlite.enabled))
                    .bind(&now_sqlite)
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
