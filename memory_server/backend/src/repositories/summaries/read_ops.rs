use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use crate::db::Db;
use crate::models::SessionSummary;

use super::collection;

pub async fn list_summaries(
    db: &Db,
    session_id: &str,
    level: Option<i64>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<SessionSummary>, String> {
    let mut filter = doc! {"session_id": session_id};
    if let Some(v) = level {
        filter.insert("level", v);
    }
    if let Some(v) = status {
        filter.insert("status", v);
    }

    let options = FindOptions::builder()
        .sort(doc! {"level": -1, "created_at": 1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_all_summaries_by_session(
    db: &Db,
    session_id: &str,
) -> Result<Vec<SessionSummary>, String> {
    let options = FindOptions::builder().sort(doc! {"created_at": 1}).build();
    let cursor = collection(db)
        .find(doc! {"session_id": session_id})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_pending_summaries_by_level_no_limit(
    db: &Db,
    session_id: &str,
    level: i64,
) -> Result<Vec<SessionSummary>, String> {
    let options = FindOptions::builder().sort(doc! {"created_at": 1}).build();
    let cursor = collection(db)
        .find(doc! {
            "session_id": session_id,
            "level": level,
            "status": "pending",
        })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}
