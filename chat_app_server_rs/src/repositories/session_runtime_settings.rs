use mongodb::bson::{doc, Bson, Document};
use serde_json::Value;
use sqlx::Row;

use crate::core::values::bool_to_sqlite_int;
use crate::models::session_runtime_settings::SessionRuntimeSettings;
use crate::repositories::db::with_db;

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|item: &String| item == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn list_from_bson(value: Option<&Bson>) -> Vec<String> {
    match value {
        Some(Bson::Array(items)) => normalize_id_list(
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect(),
        ),
        Some(Bson::String(raw)) => serde_json::from_str::<Vec<String>>(raw)
            .map(normalize_id_list)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn doc_to_settings(doc: &Document) -> Option<SessionRuntimeSettings> {
    Some(SessionRuntimeSettings {
        session_id: doc.get_str("session_id").ok()?.to_string(),
        user_id: doc.get_str("user_id").ok()?.to_string(),
        selected_model_id: doc.get_str("selected_model_id").ok().map(ToOwned::to_owned),
        selected_model_name: doc
            .get_str("selected_model_name")
            .ok()
            .map(ToOwned::to_owned),
        selected_thinking_level: doc
            .get_str("selected_thinking_level")
            .ok()
            .map(ToOwned::to_owned),
        remote_connection_id: doc
            .get_str("remote_connection_id")
            .ok()
            .map(ToOwned::to_owned),
        workspace_root: doc.get_str("workspace_root").ok().map(ToOwned::to_owned),
        mcp_enabled: doc.get_bool("mcp_enabled").unwrap_or(true),
        enabled_mcp_ids: list_from_bson(doc.get("enabled_mcp_ids")),
        auto_create_task: doc.get_bool("auto_create_task").unwrap_or(false),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

fn row_to_settings(row: sqlx::sqlite::SqliteRow) -> SessionRuntimeSettings {
    let enabled_mcp_ids_json: Option<String> = row.try_get("enabled_mcp_ids").ok();
    let enabled_mcp_ids = enabled_mcp_ids_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .map(normalize_id_list)
        .unwrap_or_default();
    let mcp_enabled_raw: Option<i64> = row.try_get("mcp_enabled").ok();
    let auto_create_task_raw: Option<i64> = row.try_get("auto_create_task").ok();

    SessionRuntimeSettings {
        session_id: row.try_get("session_id").unwrap_or_default(),
        user_id: row.try_get("user_id").unwrap_or_default(),
        selected_model_id: row.try_get("selected_model_id").ok(),
        selected_model_name: row.try_get("selected_model_name").ok(),
        selected_thinking_level: row.try_get("selected_thinking_level").ok(),
        remote_connection_id: row.try_get("remote_connection_id").ok(),
        workspace_root: row.try_get("workspace_root").ok(),
        mcp_enabled: mcp_enabled_raw.unwrap_or(1) != 0,
        enabled_mcp_ids,
        auto_create_task: auto_create_task_raw.unwrap_or(0) != 0,
        created_at: row.try_get("created_at").unwrap_or_default(),
        updated_at: row.try_get("updated_at").unwrap_or_default(),
    }
}

pub async fn get_session_runtime_settings(
    session_id: &str,
    user_id: &str,
) -> Result<Option<SessionRuntimeSettings>, String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            let user_id = user_id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("session_runtime_settings")
                    .find_one(
                        doc! { "session_id": &session_id, "user_id": &user_id },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|document| doc_to_settings(&document)))
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            let user_id = user_id.to_string();
            Box::pin(async move {
                let row = sqlx::query(
                    "SELECT * FROM session_runtime_settings WHERE session_id = ? AND user_id = ?",
                )
                .bind(&session_id)
                .bind(&user_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(row_to_settings))
            })
        },
    )
    .await
}

pub async fn upsert_session_runtime_settings(
    settings: &SessionRuntimeSettings,
) -> Result<SessionRuntimeSettings, String> {
    let now = crate::core::time::now_rfc3339();
    let mut next = settings.clone();
    next.selected_model_id = normalize_optional_text(next.selected_model_id);
    next.selected_model_name = normalize_optional_text(next.selected_model_name);
    next.selected_thinking_level = normalize_optional_text(next.selected_thinking_level);
    next.remote_connection_id = normalize_optional_text(next.remote_connection_id);
    next.workspace_root = normalize_optional_text(next.workspace_root);
    next.enabled_mcp_ids = normalize_id_list(next.enabled_mcp_ids);
    if next.created_at.trim().is_empty() {
        next.created_at = now.clone();
    }
    next.updated_at = now.clone();

    let mongo_settings = next.clone();
    let sqlite_settings = next.clone();
    with_db(
        |db| {
            Box::pin(async move {
                let enabled_mcp_ids = Bson::Array(
                    mongo_settings
                        .enabled_mcp_ids
                        .iter()
                        .map(|item| Bson::String(item.clone()))
                        .collect(),
                );
                let mut set_doc = doc! {
                    "session_id": &mongo_settings.session_id,
                    "user_id": &mongo_settings.user_id,
                    "mcp_enabled": mongo_settings.mcp_enabled,
                    "enabled_mcp_ids": enabled_mcp_ids,
                    "auto_create_task": mongo_settings.auto_create_task,
                    "updated_at": &mongo_settings.updated_at,
                };
                let mut unset_doc = Document::new();
                for (key, value) in [
                    ("selected_model_id", mongo_settings.selected_model_id.clone()),
                    ("selected_model_name", mongo_settings.selected_model_name.clone()),
                    (
                        "selected_thinking_level",
                        mongo_settings.selected_thinking_level.clone(),
                    ),
                    ("remote_connection_id", mongo_settings.remote_connection_id.clone()),
                    ("workspace_root", mongo_settings.workspace_root.clone()),
                ] {
                    if let Some(value) = value {
                        set_doc.insert(key, Bson::String(value));
                    } else {
                        unset_doc.insert(key, Bson::Int32(1));
                    }
                }
                let mut update_doc = doc! {
                    "$set": set_doc,
                    "$setOnInsert": {
                        "created_at": &mongo_settings.created_at,
                    },
                };
                if !unset_doc.is_empty() {
                    update_doc.insert("$unset", Bson::Document(unset_doc));
                }
                db.collection::<Document>("session_runtime_settings")
                    .update_one(
                        doc! {
                            "session_id": &mongo_settings.session_id,
                            "user_id": &mongo_settings.user_id,
                        },
                        update_doc,
                        mongodb::options::UpdateOptions::builder()
                            .upsert(true)
                            .build(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            Box::pin(async move {
                let enabled_mcp_ids = Value::Array(
                    sqlite_settings
                        .enabled_mcp_ids
                        .iter()
                        .map(|item| Value::String(item.clone()))
                        .collect(),
                )
                .to_string();
                sqlx::query(
                    "INSERT INTO session_runtime_settings (session_id, user_id, selected_model_id, selected_model_name, selected_thinking_level, remote_connection_id, workspace_root, mcp_enabled, enabled_mcp_ids, auto_create_task, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET user_id = excluded.user_id, selected_model_id = excluded.selected_model_id, selected_model_name = excluded.selected_model_name, selected_thinking_level = excluded.selected_thinking_level, remote_connection_id = excluded.remote_connection_id, workspace_root = excluded.workspace_root, mcp_enabled = excluded.mcp_enabled, enabled_mcp_ids = excluded.enabled_mcp_ids, auto_create_task = excluded.auto_create_task, updated_at = excluded.updated_at",
                )
                .bind(&sqlite_settings.session_id)
                .bind(&sqlite_settings.user_id)
                .bind(&sqlite_settings.selected_model_id)
                .bind(&sqlite_settings.selected_model_name)
                .bind(&sqlite_settings.selected_thinking_level)
                .bind(&sqlite_settings.remote_connection_id)
                .bind(&sqlite_settings.workspace_root)
                .bind(bool_to_sqlite_int(sqlite_settings.mcp_enabled))
                .bind(enabled_mcp_ids)
                .bind(bool_to_sqlite_int(sqlite_settings.auto_create_task))
                .bind(&sqlite_settings.created_at)
                .bind(&sqlite_settings.updated_at)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await?;

    Ok(next)
}
