// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::session_runtime_settings::SessionRuntimeSettings;
use crate::repositories::db::{mongo_find_one_doc, mongo_update_one_doc, with_db};
use mongodb::bson::{doc, Bson, Document};

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
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
        reasoning_enabled: doc.get_bool("reasoning_enabled").unwrap_or(false),
        plan_mode_enabled: doc.get_bool("plan_mode_enabled").unwrap_or(false),
        auto_create_task: doc.get_bool("auto_create_task").unwrap_or(false),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

pub async fn get_session_runtime_settings(
    session_id: &str,
    user_id: &str,
) -> Result<Option<SessionRuntimeSettings>, String> {
    with_db(|db| {
        let session_id = session_id.to_string();
        let user_id = user_id.to_string();
        Box::pin(async move {
            let doc = mongo_find_one_doc(
                db,
                "session_runtime_settings",
                doc! { "session_id": &session_id, "user_id": &user_id },
            )
            .await?;
            Ok(doc.and_then(|document| doc_to_settings(&document)))
        })
    })
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
    if next.created_at.trim().is_empty() {
        next.created_at = now.clone();
    }
    next.updated_at = now.clone();

    let mongo_settings = next.clone();
    with_db(|db| {
        Box::pin(async move {
            let mut set_doc = doc! {
                "session_id": &mongo_settings.session_id,
                "user_id": &mongo_settings.user_id,
                "reasoning_enabled": mongo_settings.reasoning_enabled,
                "plan_mode_enabled": mongo_settings.plan_mode_enabled,
                "auto_create_task": mongo_settings.auto_create_task,
                "updated_at": &mongo_settings.updated_at,
            };
            let mut unset_doc = Document::new();
            for (key, value) in [
                (
                    "selected_model_id",
                    mongo_settings.selected_model_id.clone(),
                ),
                (
                    "selected_model_name",
                    mongo_settings.selected_model_name.clone(),
                ),
                (
                    "selected_thinking_level",
                    mongo_settings.selected_thinking_level.clone(),
                ),
                (
                    "remote_connection_id",
                    mongo_settings.remote_connection_id.clone(),
                ),
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
            mongo_update_one_doc(
                db,
                "session_runtime_settings",
                doc! {
                    "session_id": &mongo_settings.session_id,
                    "user_id": &mongo_settings.user_id,
                },
                update_doc,
                Some(
                    mongodb::options::UpdateOptions::builder()
                        .upsert(true)
                        .build(),
                ),
            )
            .await?;
            Ok(())
        })
    })
    .await?;

    Ok(next)
}
