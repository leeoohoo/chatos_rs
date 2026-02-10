use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};

use crate::models::session_summary_message::{SessionSummaryMessage, SessionSummaryMessageRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<SessionSummaryMessage> {
    let id = doc.get_str("id").ok()?.to_string();
    let summary_id = doc.get_str("summary_id").ok()?.to_string();
    let session_id = doc.get_str("session_id").ok()?.to_string();
    let message_id = doc.get_str("message_id").ok()?.to_string();
    let created_at = doc.get_str("created_at").ok().unwrap_or("").to_string();
    Some(SessionSummaryMessage {
        id,
        summary_id,
        session_id,
        message_id,
        created_at,
    })
}

pub async fn create_summary_message_links(
    summary_id: &str,
    session_id: &str,
    message_ids: &[String],
) -> Result<usize, String> {
    if message_ids.is_empty() {
        return Ok(0);
    }

    let summary_id = summary_id.to_string();
    let session_id = session_id.to_string();
    let ids: Vec<String> = message_ids.iter().cloned().collect();

    with_db(
        |db| {
            let summary_id = summary_id.clone();
            let session_id = session_id.clone();
            let ids = ids.clone();
            Box::pin(async move {
                let now = chrono::Utc::now().to_rfc3339();
                let count = ids.len();
                let docs: Vec<Document> = ids.into_iter().map(|mid| {
                    to_doc(doc_from_pairs(vec![
                        ("id", Bson::String(uuid::Uuid::new_v4().to_string())),
                        ("summary_id", Bson::String(summary_id.clone())),
                        ("session_id", Bson::String(session_id.clone())),
                        ("message_id", Bson::String(mid)),
                        ("created_at", Bson::String(now.clone())),
                    ]))
                }).collect();
                if !docs.is_empty() {
                    db.collection::<Document>("session_summary_messages").insert_many(docs, None).await.map_err(|e| e.to_string())?;
                }
                Ok(count)
            })
        },
        |pool| {
            let summary_id = summary_id.clone();
            let session_id = session_id.clone();
            let ids = ids.clone();
            Box::pin(async move {
                let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
                let now = chrono::Utc::now().to_rfc3339();
                for mid in ids.iter() {
                    let record = SessionSummaryMessage::new(summary_id.clone(), session_id.clone(), mid.clone());
                    sqlx::query("INSERT INTO session_summary_messages (id, summary_id, session_id, message_id, created_at) VALUES (?, ?, ?, ?, ?)")
                        .bind(&record.id)
                        .bind(&record.summary_id)
                        .bind(&record.session_id)
                        .bind(&record.message_id)
                        .bind(&now)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                tx.commit().await.map_err(|e| e.to_string())?;
                Ok(ids.len())
            })
        }
    ).await
}

pub async fn list_summary_messages_by_summary(
    summary_id: &str,
) -> Result<Vec<SessionSummaryMessage>, String> {
    with_db(
        |db| {
            let summary_id = summary_id.to_string();
            Box::pin(async move {
                let mut cursor = db
                    .collection::<Document>("session_summary_messages")
                    .find(doc! { "summary_id": summary_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut docs = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    docs.push(doc);
                }
                let items: Vec<SessionSummaryMessage> = docs
                    .into_iter()
                    .filter_map(|d| normalize_from_doc(&d))
                    .collect();
                Ok(items)
            })
        },
        |pool| {
            let summary_id = summary_id.to_string();
            Box::pin(async move {
                let rows = sqlx::query_as::<_, SessionSummaryMessageRow>(
                    "SELECT * FROM session_summary_messages WHERE summary_id = ?",
                )
                .bind(&summary_id)
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_summary_message()).collect())
            })
        },
    )
    .await
}
