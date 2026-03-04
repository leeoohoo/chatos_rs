use futures::StreamExt;
use mongodb::bson::{doc, Bson, Document};
use serde_json::Value;
use sqlx::Row;

use crate::core::mongo_cursor::{apply_offset_limit, collect_map_sorted_desc};
use crate::core::mongo_query::insert_optional_user_id;
use crate::core::sql_query::{append_limit_offset_clause, append_optional_user_id_filter};
use crate::core::update_fields::{
    mongo_set_doc_from_optional_strings, sqlite_update_parts_from_optional_strings,
};
use crate::models::session::{Session, SessionRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn active_session_filter() -> Document {
    doc! {
        "$or": [
            { "status": "active" },
            { "status": { "$exists": false } },
            { "status": Bson::Null }
        ]
    }
}

fn active_or_archiving_session_filter() -> Document {
    doc! {
        "$or": [
            { "status": "active" },
            { "status": "archiving" },
            { "status": { "$exists": false } },
            { "status": Bson::Null }
        ]
    }
}

fn normalize_from_doc(doc: &Document) -> Option<Session> {
    let id = doc.get_str("id").ok()?.to_string();
    let title = doc.get_str("title").ok()?.to_string();
    let description = doc.get_str("description").ok().map(|s| s.to_string());
    let metadata = doc
        .get_str("metadata")
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    let user_id = doc.get_str("user_id").ok().map(|s| s.to_string());
    let project_id = doc.get_str("project_id").ok().map(|s| s.to_string());
    let status = doc
        .get_str("status")
        .ok()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "active".to_string());
    let archived_at = doc.get_str("archived_at").ok().map(|s| s.to_string());
    let created_at = doc.get_str("created_at").ok().unwrap_or("").to_string();
    let updated_at = doc.get_str("updated_at").ok().unwrap_or("").to_string();
    Some(Session {
        id,
        title,
        description,
        metadata,
        user_id,
        project_id,
        status,
        archived_at,
        created_at,
        updated_at,
    })
}

pub async fn create_session(data: &Session) -> Result<String, String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let metadata_str = data.metadata.as_ref().map(|m| m.to_string());
    let data_mongo = data.clone();
    let data_sqlite = data.clone();
    let metadata_mongo = metadata_str.clone();
    let metadata_sqlite = metadata_str.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(data_mongo.id.clone())),
                ("title", Bson::String(data_mongo.title.clone())),
                ("description", crate::core::values::optional_string_bson(data_mongo.description.clone())),
                ("metadata", crate::core::values::optional_string_bson(metadata_mongo.clone())),
                ("user_id", crate::core::values::optional_string_bson(data_mongo.user_id.clone())),
                ("project_id", crate::core::values::optional_string_bson(data_mongo.project_id.clone())),
                ("status", Bson::String("active".to_string())),
                ("archived_at", Bson::Null),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("sessions").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(data_mongo.id.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO sessions (id, title, description, metadata, user_id, project_id, status, archived_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&data_sqlite.id)
                    .bind(&data_sqlite.title)
                    .bind(&data_sqlite.description)
                    .bind(metadata_sqlite.as_deref())
                    .bind(&data_sqlite.user_id)
                    .bind(&data_sqlite.project_id)
                    .bind("active")
                    .bind(None::<String>)
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(data_sqlite.id.clone())
            })
        }
    ).await
}

pub async fn get_session_by_id(id: &str) -> Result<Option<Session>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut filter = active_session_filter();
                filter.insert("id", id);
                let doc = db
                    .collection::<Document>("sessions")
                    .find_one(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_from_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, SessionRow>(
                    "SELECT * FROM sessions WHERE id = ? AND (status = 'active' OR status IS NULL)",
                )
                .bind(&id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_session()))
            })
        },
    )
    .await
}

pub async fn get_all_sessions(limit: Option<i64>, offset: i64) -> Result<Vec<Session>, String> {
    with_db(
        |db| {
            Box::pin(async move {
                let cursor = db
                    .collection::<Document>("sessions")
                    .find(active_session_filter(), None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut sessions: Vec<Session> =
                    collect_map_sorted_desc(cursor, normalize_from_doc, |s| s.created_at.as_str())
                        .await?;
                sessions = apply_offset_limit(sessions, offset, limit);
                Ok(sessions)
            })
        },
        |pool| {
            Box::pin(async move {
                let mut query =
                    "SELECT * FROM sessions WHERE (status = 'active' OR status IS NULL) ORDER BY created_at DESC"
                        .to_string();
                append_limit_offset_clause(&mut query, limit, offset);
                if let Some(l) = limit {
                    let mut q = sqlx::query_as::<_, SessionRow>(&query).bind(l);
                    if offset > 0 {
                        q = q.bind(offset);
                    }
                    let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                    return Ok(rows.into_iter().map(|r| r.to_session()).collect());
                }
                let rows = sqlx::query_as::<_, SessionRow>(&query)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_session()).collect())
            })
        },
    )
    .await
}

pub async fn get_sessions_by_user_project(
    user_id: Option<String>,
    project_id: Option<String>,
    limit: Option<i64>,
    offset: i64,
    include_archived: bool,
    include_archiving: bool,
) -> Result<Vec<Session>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            let project_id = project_id.clone();
            let include_archived = include_archived;
            let include_archiving = include_archiving;
            Box::pin(async move {
                let mut filter = if include_archived {
                    doc! {}
                } else if include_archiving {
                    active_or_archiving_session_filter()
                } else {
                    active_session_filter()
                };
                insert_optional_user_id(&mut filter, user_id);
                if let Some(pid) = project_id {
                    filter.insert("project_id", pid);
                }
                let cursor = db
                    .collection::<Document>("sessions")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut sessions: Vec<Session> =
                    collect_map_sorted_desc(cursor, normalize_from_doc, |s| s.created_at.as_str())
                        .await?;
                sessions = apply_offset_limit(sessions, offset, limit);
                Ok(sessions)
            })
        },
        |pool| {
            let user_id = user_id.clone();
            let project_id = project_id.clone();
            let include_archived = include_archived;
            let include_archiving = include_archiving;
            Box::pin(async move {
                let mut query = if include_archived {
                    "SELECT * FROM sessions WHERE 1 = 1".to_string()
                } else if include_archiving {
                    "SELECT * FROM sessions WHERE ((status = 'active' OR status IS NULL) OR status = 'archiving')"
                        .to_string()
                } else {
                    "SELECT * FROM sessions WHERE (status = 'active' OR status IS NULL)".to_string()
                };
                let mut binds: Vec<String> = Vec::new();
                let has_user_filter = user_id.is_some();
                append_optional_user_id_filter(&mut query, has_user_filter, true);
                if let Some(uid) = user_id {
                    binds.push(uid);
                }
                if let Some(pid) = project_id {
                    query.push_str(" AND project_id = ?");
                    binds.push(pid);
                }
                query.push_str(" ORDER BY created_at DESC");
                append_limit_offset_clause(&mut query, limit, offset);
                if let Some(l) = limit {
                    let mut q = sqlx::query_as::<_, SessionRow>(&query);
                    for b in &binds {
                        q = q.bind(b);
                    }
                    q = q.bind(l);
                    if offset > 0 {
                        q = q.bind(offset);
                    }
                    let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                    return Ok(rows.into_iter().map(|r| r.to_session()).collect());
                }
                let mut q = sqlx::query_as::<_, SessionRow>(&query);
                for b in &binds {
                    q = q.bind(b);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_session()).collect())
            })
        },
    )
    .await
}

async fn archive_collection_docs_mongo(
    db: &mongodb::Database,
    source_name: &str,
    archive_name: &str,
    session_id: &str,
    archived_at: &str,
) -> Result<(), String> {
    let source = db.collection::<Document>(source_name);
    let archive = db.collection::<Document>(archive_name);
    let mut cursor = source
        .find(doc! { "session_id": session_id }, None)
        .await
        .map_err(|e| e.to_string())?;

    while let Some(item) = cursor.next().await {
        let mut record = item.map_err(|e| e.to_string())?;
        record.insert("archived_at", archived_at.to_string());
        if let Ok(id) = record.get_str("id") {
            let id = id.to_string();
            archive
                .update_one(
                    doc! { "id": id },
                    doc! { "$setOnInsert": record },
                    mongodb::options::UpdateOptions::builder()
                        .upsert(true)
                        .build(),
                )
                .await
                .map_err(|e| e.to_string())?;
        } else {
            archive
                .insert_one(record, None)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

async fn archive_session_mongo(db: &mongodb::Database, id: &str) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let session = db
        .collection::<Document>("sessions")
        .find_one(doc! { "id": id }, None)
        .await
        .map_err(|e| e.to_string())?;
    let Some(session_doc) = session else {
        return Ok(());
    };

    if session_doc
        .get_str("status")
        .ok()
        .map(|value| value.eq_ignore_ascii_case("archived"))
        .unwrap_or(false)
    {
        return Ok(());
    }

    db.collection::<Document>("sessions")
        .update_one(
            doc! { "id": id },
            doc! { "$set": { "status": "archiving", "updated_at": now.clone() } },
            None,
        )
        .await
        .map_err(|e| e.to_string())?;

    archive_collection_docs_mongo(db, "messages", "archived_messages", id, &now).await?;
    archive_collection_docs_mongo(
        db,
        "session_summaries",
        "archived_session_summaries",
        id,
        &now,
    )
    .await?;
    archive_collection_docs_mongo(
        db,
        "session_summaries_v2",
        "archived_session_summaries_v2",
        id,
        &now,
    )
    .await?;
    archive_collection_docs_mongo(
        db,
        "session_summary_messages",
        "archived_session_summary_messages",
        id,
        &now,
    )
    .await?;

    db.collection::<Document>("session_summary_messages")
        .delete_many(doc! { "session_id": id }, None)
        .await
        .map_err(|e| e.to_string())?;
    db.collection::<Document>("session_summaries_v2")
        .delete_many(doc! { "session_id": id }, None)
        .await
        .map_err(|e| e.to_string())?;
    db.collection::<Document>("session_summaries")
        .delete_many(doc! { "session_id": id }, None)
        .await
        .map_err(|e| e.to_string())?;
    db.collection::<Document>("messages")
        .delete_many(doc! { "session_id": id }, None)
        .await
        .map_err(|e| e.to_string())?;

    db.collection::<Document>("sessions")
        .update_one(
            doc! { "id": id },
            doc! {
                "$set": {
                    "status": "archived",
                    "archived_at": now.clone(),
                    "updated_at": now,
                }
            },
            None,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

async fn archive_session_sqlite(pool: &sqlx::SqlitePool, id: &str) -> Result<(), String> {
    let status_row = sqlx::query("SELECT status FROM sessions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    let Some(status_row) = status_row else {
        return Ok(());
    };

    let status: String = status_row
        .try_get("status")
        .unwrap_or_else(|_| "active".to_string());
    if status.eq_ignore_ascii_case("archived") {
        return Ok(());
    }

    let now = crate::core::time::now_rfc3339();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    sqlx::query("UPDATE sessions SET status = 'archiving', updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT OR IGNORE INTO archived_messages (id, session_id, role, content, message_mode, message_source, summary, tool_calls, tool_call_id, reasoning, metadata, summary_status, summary_id, summarized_at, created_at, archived_at)
         SELECT id, session_id, role, content, message_mode, message_source, summary, tool_calls, tool_call_id, reasoning, metadata, summary_status, summary_id, summarized_at, created_at, ?
         FROM messages WHERE session_id = ?",
    )
    .bind(&now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT OR IGNORE INTO archived_session_summaries (id, session_id, summary_text, summary_prompt, model, temperature, target_summary_tokens, keep_last_n, message_count, approx_tokens, first_message_id, last_message_id, first_message_created_at, last_message_created_at, metadata, created_at, updated_at, archived_at)
         SELECT id, session_id, summary_text, summary_prompt, model, temperature, target_summary_tokens, keep_last_n, message_count, approx_tokens, first_message_id, last_message_id, first_message_created_at, last_message_created_at, metadata, created_at, updated_at, ?
         FROM session_summaries WHERE session_id = ?",
    )
    .bind(&now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT OR IGNORE INTO archived_session_summaries_v2 (id, session_id, summary_text, summary_model, trigger_type, source_start_message_id, source_end_message_id, source_message_count, source_estimated_tokens, status, error_message, created_at, updated_at, archived_at)
         SELECT id, session_id, summary_text, summary_model, trigger_type, source_start_message_id, source_end_message_id, source_message_count, source_estimated_tokens, status, error_message, created_at, updated_at, ?
         FROM session_summaries_v2 WHERE session_id = ?",
    )
    .bind(&now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT OR IGNORE INTO archived_session_summary_messages (id, summary_id, session_id, message_id, created_at, archived_at)
         SELECT id, summary_id, session_id, message_id, created_at, ?
         FROM session_summary_messages WHERE session_id = ?",
    )
    .bind(&now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM session_summary_messages WHERE session_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM session_summaries_v2 WHERE session_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM session_summaries WHERE session_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM messages WHERE session_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query(
        "UPDATE sessions SET status = 'archived', archived_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn mark_session_archiving(id: &str) -> Result<bool, String> {
    let now = crate::core::time::now_rfc3339();
    with_db(
        |db| {
            let id = id.to_string();
            let now = now.clone();
            Box::pin(async move {
                let result = db
                    .collection::<Document>("sessions")
                    .update_one(
                        doc! {
                            "id": &id,
                            "$or": [
                                { "status": "active" },
                                { "status": { "$exists": false } },
                                { "status": Bson::Null }
                            ]
                        },
                        doc! { "$set": { "status": "archiving", "updated_at": now } },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                if result.matched_count > 0 {
                    return Ok(true);
                }
                let exists = db
                    .collection::<Document>("sessions")
                    .find_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?
                    .is_some();
                Ok(exists)
            })
        },
        |pool| {
            let id = id.to_string();
            let now = now.clone();
            Box::pin(async move {
                let result = sqlx::query(
                    "UPDATE sessions SET status = 'archiving', updated_at = ? WHERE id = ? AND (status = 'active' OR status IS NULL)",
                )
                .bind(&now)
                .bind(&id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                if result.rows_affected() > 0 {
                    return Ok(true);
                }
                let row = sqlx::query("SELECT id FROM sessions WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.is_some())
            })
        },
    )
    .await
}

pub async fn list_archiving_session_ids(limit: Option<i64>) -> Result<Vec<String>, String> {
    with_db(
        |db| {
            Box::pin(async move {
                let mut find_options = mongodb::options::FindOptions::default();
                find_options.sort = Some(doc! { "updated_at": 1 });
                if let Some(value) = limit {
                    if value > 0 {
                        find_options.limit = Some(value);
                    }
                }
                let mut cursor = db
                    .collection::<Document>("sessions")
                    .find(doc! { "status": "archiving" }, find_options)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                while let Some(item) = cursor.next().await {
                    let doc = item.map_err(|e| e.to_string())?;
                    if let Ok(id) = doc.get_str("id") {
                        out.push(id.to_string());
                    }
                }
                Ok(out)
            })
        },
        |pool| {
            Box::pin(async move {
                let query = if let Some(value) = limit {
                    if value > 0 {
                        "SELECT id FROM sessions WHERE status = 'archiving' ORDER BY updated_at ASC LIMIT ?"
                            .to_string()
                    } else {
                        "SELECT id FROM sessions WHERE status = 'archiving' ORDER BY updated_at ASC"
                            .to_string()
                    }
                } else {
                    "SELECT id FROM sessions WHERE status = 'archiving' ORDER BY updated_at ASC"
                        .to_string()
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
                    .filter_map(|row| row.try_get::<String, _>("id").ok())
                    .collect())
            })
        },
    )
    .await
}

pub async fn process_session_archive(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move { archive_session_mongo(db, &id).await })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move { archive_session_sqlite(pool, &id).await })
        },
    )
    .await
}

pub async fn delete_session(id: &str) -> Result<(), String> {
    match mark_session_archiving(id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("会话不存在".to_string()),
        Err(err) => Err(err),
    }
}

pub async fn update_session(
    id: &str,
    title: Option<String>,
    description: Option<String>,
    metadata: Option<Value>,
) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let metadata_str = metadata.as_ref().map(|m| m.to_string());
    let title_mongo = title.clone();
    let title_sqlite = title.clone();
    let description_mongo = description.clone();
    let description_sqlite = description.clone();
    let metadata_mongo = metadata_str.clone();
    let metadata_sqlite = metadata_str.clone();

    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_doc = mongo_set_doc_from_optional_strings([
                    ("title", title_mongo.clone()),
                    ("description", description_mongo.clone()),
                    ("metadata", metadata_mongo.clone()),
                ]);
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("sessions")
                    .update_one(doc! { "id": &id }, doc! { "$set": set_doc }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let (mut set_clause, binds) = sqlite_update_parts_from_optional_strings([
                    ("title", title_sqlite),
                    ("description", description_sqlite),
                    ("metadata", metadata_sqlite),
                ]);
                set_clause.push("updated_at = ?".to_string());
                let query = format!("UPDATE sessions SET {} WHERE id = ?", set_clause.join(", "));
                let mut q = sqlx::query(&query);
                for bind in binds {
                    q = q.bind(bind);
                }
                q = q.bind(&now_sqlite);
                q = q.bind(&id);
                q.execute(pool).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}
