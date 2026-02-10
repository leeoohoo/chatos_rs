use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};

use crate::models::session_summary::{SessionSummary, SessionSummaryRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<SessionSummary> {
    let id = doc.get_str("id").ok()?.to_string();
    let session_id = doc.get_str("session_id").ok()?.to_string();
    let summary_text = doc.get_str("summary_text").ok()?.to_string();
    let summary_prompt = doc.get_str("summary_prompt").ok().map(|s| s.to_string());
    let model = doc.get_str("model").ok().map(|s| s.to_string());
    let temperature = doc.get_f64("temperature").ok();
    let target_summary_tokens = doc.get_i64("target_summary_tokens").ok();
    let keep_last_n = doc.get_i64("keep_last_n").ok();
    let message_count = doc.get_i64("message_count").ok();
    let approx_tokens = doc.get_i64("approx_tokens").ok();
    let first_message_id = doc.get_str("first_message_id").ok().map(|s| s.to_string());
    let last_message_id = doc.get_str("last_message_id").ok().map(|s| s.to_string());
    let first_message_created_at = doc
        .get_str("first_message_created_at")
        .ok()
        .map(|s| s.to_string());
    let last_message_created_at = doc
        .get_str("last_message_created_at")
        .ok()
        .map(|s| s.to_string());
    let metadata = doc
        .get_str("metadata")
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
    let created_at = doc.get_str("created_at").ok().unwrap_or("").to_string();
    let updated_at = doc.get_str("updated_at").ok().unwrap_or("").to_string();
    Some(SessionSummary {
        id,
        session_id,
        summary_text,
        summary_prompt,
        model,
        temperature,
        target_summary_tokens,
        keep_last_n,
        message_count,
        approx_tokens,
        first_message_id,
        last_message_id,
        first_message_created_at,
        last_message_created_at,
        metadata,
        created_at,
        updated_at,
    })
}

pub async fn create_summary(data: &SessionSummary) -> Result<SessionSummary, String> {
    let data_mongo = data.clone();
    let data_sqlite = data.clone();
    let metadata_str = data.metadata.as_ref().map(|v| v.to_string());
    let metadata_mongo = metadata_str.clone();
    let metadata_sqlite = metadata_str.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(data_mongo.id.clone())),
                ("session_id", Bson::String(data_mongo.session_id.clone())),
                ("summary_text", Bson::String(data_mongo.summary_text.clone())),
                ("summary_prompt", data_mongo.summary_prompt.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("model", data_mongo.model.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("temperature", data_mongo.temperature.map(Bson::Double).unwrap_or(Bson::Null)),
                ("target_summary_tokens", data_mongo.target_summary_tokens.map(Bson::Int64).unwrap_or(Bson::Null)),
                ("keep_last_n", data_mongo.keep_last_n.map(Bson::Int64).unwrap_or(Bson::Null)),
                ("message_count", data_mongo.message_count.map(Bson::Int64).unwrap_or(Bson::Null)),
                ("approx_tokens", data_mongo.approx_tokens.map(Bson::Int64).unwrap_or(Bson::Null)),
                ("first_message_id", data_mongo.first_message_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("last_message_id", data_mongo.last_message_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("first_message_created_at", data_mongo.first_message_created_at.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("last_message_created_at", data_mongo.last_message_created_at.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("metadata", metadata_mongo.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("created_at", Bson::String(data_mongo.created_at.clone())),
                ("updated_at", Bson::String(data_mongo.updated_at.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("session_summaries").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(data_mongo.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO session_summaries (id, session_id, summary_text, summary_prompt, model, temperature, target_summary_tokens, keep_last_n, message_count, approx_tokens, first_message_id, last_message_id, first_message_created_at, last_message_created_at, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&data_sqlite.id)
                    .bind(&data_sqlite.session_id)
                    .bind(&data_sqlite.summary_text)
                    .bind(&data_sqlite.summary_prompt)
                    .bind(&data_sqlite.model)
                    .bind(&data_sqlite.temperature)
                    .bind(&data_sqlite.target_summary_tokens)
                    .bind(&data_sqlite.keep_last_n)
                    .bind(&data_sqlite.message_count)
                    .bind(&data_sqlite.approx_tokens)
                    .bind(&data_sqlite.first_message_id)
                    .bind(&data_sqlite.last_message_id)
                    .bind(&data_sqlite.first_message_created_at)
                    .bind(&data_sqlite.last_message_created_at)
                    .bind(metadata_sqlite.as_deref())
                    .bind(&data_sqlite.created_at)
                    .bind(&data_sqlite.updated_at)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(data_sqlite.clone())
            })
        }
    ).await
}

pub async fn list_summaries_by_session(
    session_id: &str,
    limit: Option<i64>,
) -> Result<Vec<SessionSummary>, String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let mut options = mongodb::options::FindOptions::default();
                options.sort = Some(doc! { "created_at": 1 });
                if let Some(l) = limit { options.limit = Some(l); }
                let mut cursor = db.collection::<Document>("session_summaries").find(doc! { "session_id": session_id }, options).await.map_err(|e| e.to_string())?;
                let mut docs = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    docs.push(doc);
                }
                let mut items: Vec<SessionSummary> = docs.into_iter().filter_map(|d| normalize_from_doc(&d)).collect();
                items.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                Ok(items)
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let rows = if let Some(l) = limit {
                    sqlx::query_as::<_, SessionSummaryRow>("SELECT * FROM session_summaries WHERE session_id = ? ORDER BY created_at ASC LIMIT ?")
                        .bind(&session_id)
                        .bind(l)
                        .fetch_all(pool)
                        .await
                        .map_err(|e| e.to_string())?
                } else {
                    sqlx::query_as::<_, SessionSummaryRow>("SELECT * FROM session_summaries WHERE session_id = ? ORDER BY created_at ASC")
                        .bind(&session_id)
                        .fetch_all(pool)
                        .await
                        .map_err(|e| e.to_string())?
                };
                Ok(rows.into_iter().map(|r| r.to_summary()).collect())
            })
        }
    ).await
}

pub async fn get_last_summary_by_session(
    session_id: &str,
) -> Result<Option<SessionSummary>, String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let options = mongodb::options::FindOneOptions::builder().sort(doc! { "created_at": -1 }).build();
                let doc = db.collection::<Document>("session_summaries").find_one(doc! { "session_id": session_id }, options).await.map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_from_doc(&d)))
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, SessionSummaryRow>("SELECT * FROM session_summaries WHERE session_id = ? ORDER BY created_at DESC LIMIT 1")
                    .bind(&session_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_summary()))
            })
        }
    ).await
}
