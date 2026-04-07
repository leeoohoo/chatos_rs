use mongodb::bson::doc;
use tracing::info;

use super::index_helpers::{ensure_index, ensure_unique_index};
use super::Db;

pub async fn init_schema(db: &Db) -> Result<(), String> {
    ensure_user_indexes(db).await?;
    ensure_contact_indexes(db).await?;
    ensure_conversation_indexes(db).await?;
    ensure_conversation_message_indexes(db).await?;
    ensure_action_request_indexes(db).await?;
    ensure_run_indexes(db).await?;

    info!("[IM-SERVICE] mongodb indexes initialized");
    Ok(())
}

async fn ensure_user_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("users"), doc! {"id": 1}).await?;
    ensure_unique_index(db.collection("users"), doc! {"username": 1}).await?;
    ensure_index(db.collection("users"), doc! {"status": 1, "created_at": -1}).await?;
    Ok(())
}

async fn ensure_contact_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("contacts"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("contacts"),
        doc! {"owner_user_id": 1, "status": 1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("contacts"),
        doc! {"owner_user_id": 1, "agent_id": 1},
    )
    .await?;
    Ok(())
}

async fn ensure_conversation_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("conversations"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("conversations"),
        doc! {"owner_user_id": 1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("conversations"),
        doc! {"contact_id": 1, "updated_at": -1},
    )
    .await?;
    Ok(())
}

async fn ensure_conversation_message_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("conversation_messages"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("conversation_messages"),
        doc! {"conversation_id": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        db.collection("conversation_messages"),
        doc! {"delivery_status": 1, "created_at": -1},
    )
    .await?;
    Ok(())
}

async fn ensure_action_request_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("conversation_action_requests"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("conversation_action_requests"),
        doc! {"conversation_id": 1, "status": 1, "created_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("conversation_action_requests"),
        doc! {"run_id": 1, "created_at": -1},
    )
    .await?;
    Ok(())
}

async fn ensure_run_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("conversation_runs"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("conversation_runs"),
        doc! {"conversation_id": 1, "created_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("conversation_runs"),
        doc! {"status": 1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("conversation_runs"),
        doc! {"source_message_id": 1},
    )
    .await?;
    Ok(())
}
