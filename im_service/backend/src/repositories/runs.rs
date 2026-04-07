use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{ConversationRun, CreateConversationRunRequest, UpdateConversationRunRequest};

use super::{normalize_optional_text, now_rfc3339};

fn collection(db: &Db) -> mongodb::Collection<ConversationRun> {
    db.collection::<ConversationRun>("conversation_runs")
}

pub async fn create_run(db: &Db, req: CreateConversationRunRequest) -> Result<ConversationRun, String> {
    let now = now_rfc3339();
    let run = ConversationRun {
        id: Uuid::new_v4().to_string(),
        conversation_id: req.conversation_id.trim().to_string(),
        source_message_id: req.source_message_id.trim().to_string(),
        contact_id: req.contact_id.trim().to_string(),
        agent_id: req.agent_id.trim().to_string(),
        project_id: normalize_optional_text(req.project_id.as_deref()),
        execution_session_id: normalize_optional_text(req.execution_session_id.as_deref()),
        execution_turn_id: normalize_optional_text(req.execution_turn_id.as_deref()),
        execution_scope_key: normalize_optional_text(req.execution_scope_key.as_deref()),
        status: req
            .status
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "pending".to_string()),
        final_message_id: None,
        error_message: None,
        started_at: normalize_optional_text(req.started_at.as_deref()),
        finished_at: None,
        created_at: now.clone(),
        updated_at: now,
    };

    collection(db)
        .insert_one(run.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(run)
}

pub async fn get_run_by_id(db: &Db, run_id: &str) -> Result<Option<ConversationRun>, String> {
    collection(db)
        .find_one(doc! {"id": run_id.trim()})
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_runs_by_conversation(
    db: &Db,
    conversation_id: &str,
    limit: i64,
) -> Result<Vec<ConversationRun>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"created_at": -1})
        .limit(Some(limit.max(1)))
        .build();

    let cursor = collection(db)
        .find(doc! {"conversation_id": conversation_id.trim()})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn update_run(
    db: &Db,
    run_id: &str,
    req: UpdateConversationRunRequest,
) -> Result<Option<ConversationRun>, String> {
    let Some(existing) = get_run_by_id(db, run_id).await? else {
        return Ok(None);
    };

    let mut update_fields = doc! {
        "updated_at": now_rfc3339(),
    };

    if let Some(status) = normalize_optional_text(req.status.as_deref()) {
        update_fields.insert("status", status);
    }
    if let Some(value) = req.final_message_id {
        update_fields.insert(
            "final_message_id",
            normalize_optional_text(Some(value.as_str()))
                .map(Bson::String)
                .unwrap_or(Bson::Null),
        );
    }
    if let Some(value) = req.error_message {
        update_fields.insert(
            "error_message",
            normalize_optional_text(Some(value.as_str()))
                .map(Bson::String)
                .unwrap_or(Bson::Null),
        );
    }
    if let Some(value) = req.execution_session_id {
        update_fields.insert(
            "execution_session_id",
            normalize_optional_text(Some(value.as_str()))
                .map(Bson::String)
                .unwrap_or(Bson::Null),
        );
    }
    if let Some(value) = req.execution_turn_id {
        update_fields.insert(
            "execution_turn_id",
            normalize_optional_text(Some(value.as_str()))
                .map(Bson::String)
                .unwrap_or(Bson::Null),
        );
    }
    if let Some(value) = req.execution_scope_key {
        update_fields.insert(
            "execution_scope_key",
            normalize_optional_text(Some(value.as_str()))
                .map(Bson::String)
                .unwrap_or(Bson::Null),
        );
    }
    if let Some(value) = req.started_at {
        update_fields.insert(
            "started_at",
            normalize_optional_text(Some(value.as_str()))
                .map(Bson::String)
                .unwrap_or(Bson::Null),
        );
    }
    if let Some(value) = req.finished_at {
        update_fields.insert(
            "finished_at",
            normalize_optional_text(Some(value.as_str()))
                .map(Bson::String)
                .unwrap_or(Bson::Null),
        );
    }

    collection(db)
        .update_one(doc! {"id": &existing.id}, doc! {"$set": update_fields})
        .await
        .map_err(|e| e.to_string())?;

    get_run_by_id(db, run_id).await
}
