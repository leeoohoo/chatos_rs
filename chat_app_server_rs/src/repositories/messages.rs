use mongodb::bson::{doc, Bson, Document};
use serde_json::Value;
use sqlx::Row;
use std::collections::HashSet;

use crate::core::mongo_cursor::{
    apply_offset_limit, collect_map_sorted_asc, collect_map_sorted_desc,
};
use crate::core::sql_query::append_limit_offset_clause;
use crate::models::message::{Message, MessageRow};
use crate::repositories::db::{doc_from_pairs, get_db_sync, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<Message> {
    let id = doc.get_str("id").ok()?.to_string();
    let session_id = doc.get_str("session_id").ok()?.to_string();
    let role = doc.get_str("role").ok()?.to_string();
    let content = doc.get_str("content").ok().unwrap_or("").to_string();
    let message_mode = doc.get_str("message_mode").ok().map(|s| s.to_string());
    let message_source = doc.get_str("message_source").ok().map(|s| s.to_string());
    let summary = doc.get_str("summary").ok().map(|s| s.to_string());
    let tool_calls = doc
        .get_str("tool_calls")
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    let tool_call_id = doc.get_str("tool_call_id").ok().map(|s| s.to_string());
    let reasoning = doc.get_str("reasoning").ok().map(|s| s.to_string());
    let metadata = doc
        .get_str("metadata")
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    let created_at = doc.get_str("created_at").ok().unwrap_or("").to_string();
    Some(Message {
        id,
        session_id,
        role,
        content,
        message_mode,
        message_source,
        summary,
        tool_calls,
        tool_call_id,
        reasoning,
        metadata,
        created_at,
    })
}

pub async fn create_message(data: &Message) -> Result<Message, String> {
    let data_mongo = data.clone();
    let data_sqlite = data.clone();
    let now = data.created_at.clone();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let tool_calls_str = data.tool_calls.as_ref().map(|v| v.to_string());
    let tool_calls_mongo = tool_calls_str.clone();
    let tool_calls_sqlite = tool_calls_str.clone();
    let metadata_str = data.metadata.as_ref().map(|v| v.to_string());
    let metadata_mongo = metadata_str.clone();
    let metadata_sqlite = metadata_str.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(data_mongo.id.clone())),
                ("session_id", Bson::String(data_mongo.session_id.clone())),
                ("role", Bson::String(data_mongo.role.clone())),
                ("content", Bson::String(data_mongo.content.clone())),
                ("message_mode", crate::core::values::optional_string_bson(data_mongo.message_mode.clone())),
                ("message_source", crate::core::values::optional_string_bson(data_mongo.message_source.clone())),
                ("summary", crate::core::values::optional_string_bson(data_mongo.summary.clone())),
                ("tool_calls", crate::core::values::optional_string_bson(tool_calls_mongo.clone())),
                ("tool_call_id", crate::core::values::optional_string_bson(data_mongo.tool_call_id.clone())),
                ("reasoning", crate::core::values::optional_string_bson(data_mongo.reasoning.clone())),
                ("metadata", crate::core::values::optional_string_bson(metadata_mongo.clone())),
                ("created_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("messages").insert_one(doc.clone(), None).await.map_err(|e| e.to_string())?;
                Ok(data_mongo.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO messages (id, session_id, role, content, message_mode, message_source, summary, tool_calls, tool_call_id, reasoning, metadata, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&data_sqlite.id)
                    .bind(&data_sqlite.session_id)
                    .bind(&data_sqlite.role)
                    .bind(&data_sqlite.content)
                    .bind(&data_sqlite.message_mode)
                    .bind(&data_sqlite.message_source)
                    .bind(&data_sqlite.summary)
                    .bind(tool_calls_sqlite.as_deref())
                    .bind(&data_sqlite.tool_call_id)
                    .bind(&data_sqlite.reasoning)
                    .bind(metadata_sqlite.as_deref())
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(data_sqlite.clone())
            })
        }
    ).await
}

pub fn create_message_sync(data: &Message) -> Result<Message, String> {
    // Only support SQLite sync via block_on
    let db = get_db_sync()?;
    if db.is_mongo() {
        return Err("MongoDB adapter does not support create_sync".to_string());
    }
    block_on(create_message(data))
}

pub async fn get_message_by_id(id: &str) -> Result<Option<Message>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("messages")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_from_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_message()))
            })
        },
    )
    .await
}

pub async fn get_messages_by_session(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<Message>, String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let cursor = db
                    .collection::<Document>("messages")
                    .find(doc! { "session_id": session_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut messages: Vec<Message> =
                    collect_map_sorted_asc(cursor, normalize_from_doc, |m| m.created_at.as_str())
                        .await?;
                if let Some(l) = limit {
                    messages = apply_offset_limit(messages, offset, Some(l));
                }
                Ok(messages)
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let mut query =
                    "SELECT * FROM messages WHERE session_id = ? ORDER BY created_at ASC"
                        .to_string();
                append_limit_offset_clause(&mut query, limit, offset);
                let mut q = sqlx::query_as::<_, MessageRow>(&query).bind(&session_id);
                if let Some(l) = limit {
                    q = q.bind(l);
                    if offset > 0 {
                        q = q.bind(offset);
                    }
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_message()).collect())
            })
        },
    )
    .await
}

pub async fn get_recent_messages_by_session(
    session_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Message>, String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let cursor = db
                    .collection::<Document>("messages")
                    .find(doc! { "session_id": session_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let messages: Vec<Message> =
                    collect_map_sorted_desc(cursor, normalize_from_doc, |m| m.created_at.as_str())
                        .await?;
                let mut out = apply_offset_limit(messages, offset, Some(limit));
                out.reverse();
                Ok(out)
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let rows = sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE session_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?")
                    .bind(&session_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out: Vec<Message> = rows.into_iter().map(|r| r.to_message()).collect();
                out.reverse();
                Ok(out)
            })
        }
    ).await
}

pub async fn get_messages_by_session_after(
    session_id: &str,
    after_created_at: &str,
    limit: Option<i64>,
) -> Result<Vec<Message>, String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            let after_created_at = after_created_at.to_string();
            Box::pin(async move {
                let mut options = mongodb::options::FindOptions::default();
                options.sort = Some(doc! { "created_at": 1 });
                if let Some(l) = limit {
                    options.limit = Some(l);
                }
                let cursor = db
                    .collection::<Document>("messages")
                    .find(doc! { "session_id": session_id, "created_at": { "$gt": after_created_at } }, options)
                    .await
                    .map_err(|e| e.to_string())?;
                let messages: Vec<Message> =
                    collect_map_sorted_asc(cursor, normalize_from_doc, |m| m.created_at.as_str())
                        .await?;
                Ok(messages)
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            let after_created_at = after_created_at.to_string();
            Box::pin(async move {
                let rows = if let Some(l) = limit {
                    sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE session_id = ? AND created_at > ? ORDER BY created_at ASC LIMIT ?")
                        .bind(&session_id)
                        .bind(&after_created_at)
                        .bind(l)
                        .fetch_all(pool)
                        .await
                        .map_err(|e| e.to_string())?
                } else {
                    sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE session_id = ? AND created_at > ? ORDER BY created_at ASC")
                        .bind(&session_id)
                        .bind(&after_created_at)
                        .fetch_all(pool)
                        .await
                        .map_err(|e| e.to_string())?
                };
                Ok(rows.into_iter().map(|r| r.to_message()).collect())
            })
        }
    ).await
}

pub async fn delete_message(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("messages")
                    .delete_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM messages WHERE id = ?")
                    .bind(&id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn delete_messages_by_session(session_id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                db.collection::<Document>("messages")
                    .delete_many(doc! { "session_id": session_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM messages WHERE session_id = ?")
                    .bind(&session_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn count_messages_by_session(session_id: &str) -> Result<i64, String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let count = db
                    .collection::<Document>("messages")
                    .count_documents(doc! { "session_id": session_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(count as i64)
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let row =
                    sqlx::query("SELECT COUNT(*) as count FROM messages WHERE session_id = ?")
                        .bind(&session_id)
                        .fetch_one(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                let count: i64 = row.try_get("count").unwrap_or(0);
                Ok(count)
            })
        },
    )
    .await
}

pub async fn list_sessions_with_pending_summary(limit: Option<i64>) -> Result<Vec<String>, String> {
    with_db(
        |db| {
            Box::pin(async move {
                let filter = doc! {
                    "$or": [
                        { "summary_status": "pending" },
                        { "summary_status": { "$exists": false } },
                        { "summary_status": Bson::Null }
                    ]
                };
                let cursor = db
                    .collection::<Document>("messages")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let messages: Vec<Message> =
                    collect_map_sorted_asc(cursor, normalize_from_doc, |m| m.created_at.as_str())
                        .await?;
                let mut session_ids: Vec<String> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                for message in messages {
                    if message.session_id.is_empty() {
                        continue;
                    }
                    if !seen.insert(message.session_id.clone()) {
                        continue;
                    }
                    session_ids.push(message.session_id);
                    if let Some(max) = limit {
                        if max > 0 && session_ids.len() as i64 >= max {
                            break;
                        }
                    }
                }
                Ok(session_ids)
            })
        },
        |pool| {
            Box::pin(async move {
                let query = if let Some(value) = limit {
                    if value > 0 {
                        "SELECT session_id FROM messages WHERE summary_status = 'pending' OR summary_status IS NULL GROUP BY session_id ORDER BY MIN(created_at) ASC LIMIT ?".to_string()
                    } else {
                        "SELECT session_id FROM messages WHERE summary_status = 'pending' OR summary_status IS NULL GROUP BY session_id ORDER BY MIN(created_at) ASC".to_string()
                    }
                } else {
                    "SELECT session_id FROM messages WHERE summary_status = 'pending' OR summary_status IS NULL GROUP BY session_id ORDER BY MIN(created_at) ASC".to_string()
                };
                let mut q = sqlx::query(&query);
                if let Some(value) = limit {
                    if value > 0 {
                        q = q.bind(value);
                    }
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows
                    .into_iter()
                    .filter_map(|row| row.try_get::<String, _>("session_id").ok())
                    .collect())
            })
        },
    )
    .await
}

pub async fn get_pending_messages_for_summary(
    session_id: &str,
    limit: Option<i64>,
) -> Result<Vec<Message>, String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let mut options = mongodb::options::FindOptions::default();
                options.sort = Some(doc! { "created_at": 1 });
                if let Some(l) = limit {
                    if l > 0 {
                        options.limit = Some(l);
                    }
                }
                let cursor = db
                    .collection::<Document>("messages")
                    .find(
                        doc! {
                            "session_id": session_id,
                            "$or": [
                                { "summary_status": "pending" },
                                { "summary_status": { "$exists": false } },
                                { "summary_status": Bson::Null }
                            ]
                        },
                        options,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                collect_map_sorted_asc(cursor, normalize_from_doc, |m| m.created_at.as_str()).await
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let rows = if let Some(l) = limit {
                    if l > 0 {
                        sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE session_id = ? AND (summary_status = 'pending' OR summary_status IS NULL) ORDER BY created_at ASC LIMIT ?")
                            .bind(&session_id)
                            .bind(l)
                            .fetch_all(pool)
                            .await
                            .map_err(|e| e.to_string())?
                    } else {
                        sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE session_id = ? AND (summary_status = 'pending' OR summary_status IS NULL) ORDER BY created_at ASC")
                            .bind(&session_id)
                            .fetch_all(pool)
                            .await
                            .map_err(|e| e.to_string())?
                    }
                } else {
                    sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE session_id = ? AND (summary_status = 'pending' OR summary_status IS NULL) ORDER BY created_at ASC")
                        .bind(&session_id)
                        .fetch_all(pool)
                        .await
                        .map_err(|e| e.to_string())?
                };
                Ok(rows.into_iter().map(|row| row.to_message()).collect())
            })
        },
    )
    .await
}

pub async fn mark_messages_summarized(
    session_id: &str,
    message_ids: &[String],
    summary_id: &str,
    summarized_at: &str,
) -> Result<usize, String> {
    if message_ids.is_empty() {
        return Ok(0);
    }

    with_db(
        |db| {
            let session_id = session_id.to_string();
            let summary_id = summary_id.to_string();
            let summarized_at = summarized_at.to_string();
            let ids: Vec<Bson> = message_ids.iter().map(|id| Bson::String(id.clone())).collect();
            Box::pin(async move {
                let result = db
                    .collection::<Document>("messages")
                    .update_many(
                        doc! {
                            "session_id": session_id,
                            "id": { "$in": ids }
                        },
                        doc! {
                            "$set": {
                                "summary_status": "summarized",
                                "summary_id": summary_id,
                                "summarized_at": summarized_at
                            }
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(result.modified_count as usize)
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            let summary_id = summary_id.to_string();
            let summarized_at = summarized_at.to_string();
            let ids: Vec<String> = message_ids.to_vec();
            Box::pin(async move {
                let placeholders = vec!["?"; ids.len()].join(", ");
                let query = format!(
                    "UPDATE messages SET summary_status = 'summarized', summary_id = ?, summarized_at = ? WHERE session_id = ? AND id IN ({})",
                    placeholders
                );
                let mut q = sqlx::query(&query)
                    .bind(&summary_id)
                    .bind(&summarized_at)
                    .bind(&session_id);
                for id in ids {
                    q = q.bind(id);
                }
                let result = q.execute(pool).await.map_err(|e| e.to_string())?;
                Ok(result.rows_affected() as usize)
            })
        },
    )
    .await
}

pub async fn reset_messages_summary_by_summary_id(
    session_id: &str,
    summary_id: &str,
) -> Result<usize, String> {
    if session_id.trim().is_empty() || summary_id.trim().is_empty() {
        return Ok(0);
    }

    with_db(
        |db| {
            let session_id = session_id.to_string();
            let summary_id = summary_id.to_string();
            Box::pin(async move {
                let result = db
                    .collection::<Document>("messages")
                    .update_many(
                        doc! {
                            "session_id": session_id,
                            "summary_id": summary_id
                        },
                        doc! {
                            "$set": {
                                "summary_status": "pending",
                                "summary_id": Bson::Null,
                                "summarized_at": Bson::Null
                            }
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(result.modified_count as usize)
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            let summary_id = summary_id.to_string();
            Box::pin(async move {
                let result = sqlx::query(
                    "UPDATE messages SET summary_status = 'pending', summary_id = NULL, summarized_at = NULL WHERE session_id = ? AND summary_id = ?",
                )
                .bind(&session_id)
                .bind(&summary_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(result.rows_affected() as usize)
            })
        },
    )
    .await
}

pub async fn reset_messages_summary_by_session(session_id: &str) -> Result<usize, String> {
    if session_id.trim().is_empty() {
        return Ok(0);
    }

    with_db(
        |db| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let result = db
                    .collection::<Document>("messages")
                    .update_many(
                        doc! {
                            "session_id": session_id,
                            "$or": [
                                { "summary_status": "summarized" },
                                { "summary_id": { "$exists": true, "$ne": Bson::Null } },
                                { "summarized_at": { "$exists": true, "$ne": Bson::Null } }
                            ]
                        },
                        doc! {
                            "$set": {
                                "summary_status": "pending",
                                "summary_id": Bson::Null,
                                "summarized_at": Bson::Null
                            }
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(result.modified_count as usize)
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let result = sqlx::query(
                    "UPDATE messages SET summary_status = 'pending', summary_id = NULL, summarized_at = NULL WHERE session_id = ? AND (summary_status = 'summarized' OR summary_id IS NOT NULL OR summarized_at IS NOT NULL)",
                )
                .bind(&session_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(result.rows_affected() as usize)
            })
        },
    )
    .await
}

fn block_on<F: std::future::Future<Output = Result<Message, String>>>(
    fut: F,
) -> Result<Message, String> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(fut))
    } else {
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(fut)
    }
}
