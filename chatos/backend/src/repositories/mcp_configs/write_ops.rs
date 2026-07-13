// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};

use crate::models::mcp_config::McpConfig;
use crate::repositories::db::{
    doc_from_pairs, mongo_delete_many_doc, mongo_delete_one_doc, mongo_insert_doc,
    mongo_update_set_doc, to_doc, with_db,
};

pub async fn create_mcp_config(cfg: &McpConfig) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let args_str = cfg.args.as_ref().map(|v| v.to_string());
    let env_str = cfg.env.as_ref().map(|v| v.to_string());
    let cfg_mongo = cfg.clone();
    let args_str_mongo = args_str.clone();
    let env_str_mongo = env_str.clone();

    with_db(|db| {
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
            mongo_insert_doc(db, "mcp_configs", doc).await?;
            Ok(())
        })
    })
    .await
}

pub async fn update_mcp_config(id: &str, updates: &McpConfig) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let args_str = updates.args.as_ref().map(|v| v.to_string());
    let env_str = updates.env.as_ref().map(|v| v.to_string());
    let updates_mongo = updates.clone();
    let args_str_mongo = args_str.clone();
    let env_str_mongo = env_str.clone();

    with_db(|db| {
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
            mongo_update_set_doc(db, "mcp_configs", doc! { "id": id }, set_doc).await?;
            Ok(())
        })
    })
    .await
}

pub async fn delete_mcp_config(id: &str) -> Result<(), String> {
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            mongo_delete_one_doc(db, "mcp_configs", doc! { "id": &id }).await?;
            mongo_delete_many_doc(db, "mcp_config_applications", doc! { "mcp_config_id": &id })
                .await?;
            mongo_delete_many_doc(db, "mcp_config_profiles", doc! { "mcp_config_id": &id }).await?;
            Ok(())
        })
    })
    .await
}
