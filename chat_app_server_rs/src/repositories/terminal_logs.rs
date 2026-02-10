use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};

use crate::models::terminal_log::{TerminalLog, TerminalLogRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_doc(doc: &Document) -> Option<TerminalLog> {
    Some(TerminalLog {
        id: doc.get_str("id").ok()?.to_string(),
        terminal_id: doc.get_str("terminal_id").ok()?.to_string(),
        log_type: doc.get_str("type").ok()?.to_string(),
        content: doc.get_str("content").ok()?.to_string(),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
    })
}

pub async fn create_terminal_log(log: &TerminalLog) -> Result<String, String> {
    let log_mongo = log.clone();
    let log_sqlite = log.clone();
    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(log_mongo.id.clone())),
                ("terminal_id", Bson::String(log_mongo.terminal_id.clone())),
                ("type", Bson::String(log_mongo.log_type.clone())),
                ("content", Bson::String(log_mongo.content.clone())),
                ("created_at", Bson::String(log_mongo.created_at.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("terminal_logs").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(log_mongo.id.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO terminal_logs (id, terminal_id, type, content, created_at) VALUES (?, ?, ?, ?, ?)")
                    .bind(&log_sqlite.id)
                    .bind(&log_sqlite.terminal_id)
                    .bind(&log_sqlite.log_type)
                    .bind(&log_sqlite.content)
                    .bind(&log_sqlite.created_at)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(log_sqlite.id.clone())
            })
        }
    ).await
}

pub async fn list_terminal_logs(
    terminal_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<TerminalLog>, String> {
    with_db(
        |db| {
            let terminal_id = terminal_id.to_string();
            let limit = limit.clone();
            Box::pin(async move {
                let mut options = mongodb::options::FindOptions::builder().sort(doc! { "created_at": 1 }).build();
                if let Some(l) = limit { options.limit = Some(l); }
                if offset > 0 { options.skip = Some(offset as u64); }
                let mut cursor = db.collection::<Document>("terminal_logs")
                    .find(doc! { "terminal_id": terminal_id }, options)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    if let Some(item) = normalize_doc(&doc) { out.push(item); }
                }
                Ok(out)
            })
        },
        |pool| {
            let terminal_id = terminal_id.to_string();
            let limit = limit.clone();
            Box::pin(async move {
                let mut query = "SELECT id, terminal_id, type as log_type, content, created_at FROM terminal_logs WHERE terminal_id = ? ORDER BY created_at ASC".to_string();
                if let Some(_l) = limit {
                    query.push_str(" LIMIT ?");
                    if offset > 0 { query.push_str(" OFFSET ?"); }
                }
                let mut q = sqlx::query_as::<_, TerminalLogRow>(&query).bind(&terminal_id);
                if let Some(l) = limit {
                    q = q.bind(l);
                    if offset > 0 { q = q.bind(offset); }
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_log()).collect())
            })
        }
    ).await
}

pub async fn list_terminal_logs_recent(
    terminal_id: &str,
    limit: i64,
) -> Result<Vec<TerminalLog>, String> {
    let capped_limit = limit.max(1);
    with_db(
        |db| {
            let terminal_id = terminal_id.to_string();
            Box::pin(async move {
                let options = mongodb::options::FindOptions::builder()
                    .sort(doc! { "created_at": -1 })
                    .limit(Some(capped_limit))
                    .build();
                let mut cursor = db
                    .collection::<Document>("terminal_logs")
                    .find(doc! { "terminal_id": terminal_id }, options)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    if let Some(item) = normalize_doc(&doc) {
                        out.push(item);
                    }
                }
                out.reverse();
                Ok(out)
            })
        },
        |pool| {
            let terminal_id = terminal_id.to_string();
            Box::pin(async move {
                let rows = sqlx::query_as::<_, TerminalLogRow>(
                    "SELECT id, terminal_id, type as log_type, content, created_at FROM terminal_logs WHERE terminal_id = ? ORDER BY created_at DESC LIMIT ?",
                )
                .bind(&terminal_id)
                .bind(capped_limit)
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
                let mut logs: Vec<TerminalLog> = rows.into_iter().map(|r| r.to_log()).collect();
                logs.reverse();
                Ok(logs)
            })
        },
    )
    .await
}

pub async fn delete_terminal_logs(terminal_id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let terminal_id = terminal_id.to_string();
            Box::pin(async move {
                db.collection::<Document>("terminal_logs")
                    .delete_many(doc! { "terminal_id": &terminal_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let terminal_id = terminal_id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM terminal_logs WHERE terminal_id = ?")
                    .bind(&terminal_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}
