use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use crate::db::Db;
use crate::models::Message;

use super::collection;

pub async fn get_message_by_id(db: &Db, message_id: &str) -> Result<Option<Message>, String> {
    collection(db)
        .find_one(doc! {"id": message_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_messages_by_session(
    db: &Db,
    session_id: &str,
    limit: i64,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    let sort_order = if asc { 1 } else { -1 };
    let options = FindOptions::builder()
        .sort(doc! {"created_at": sort_order})
        .limit(Some(limit.max(1).min(2000)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = collection(db)
        .find(doc! {"session_id": session_id})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_pending_messages(
    db: &Db,
    session_id: &str,
    limit: Option<i64>,
) -> Result<Vec<Message>, String> {
    let options = if let Some(v) = limit {
        FindOptions::builder()
            .sort(doc! {"created_at": 1})
            .limit(Some(v.max(1)))
            .build()
    } else {
        FindOptions::builder().sort(doc! {"created_at": 1}).build()
    };

    let cursor = collection(db)
        .find(doc! {"session_id": session_id, "summary_status": "pending"})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}
