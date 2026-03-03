use std::collections::HashSet;

use mongodb::bson::{doc, Bson, Document};
use sqlx::Row;

use crate::core::mongo_cursor::collect_map_sorted_asc;
use crate::models::sub_agent_run_message::{SubAgentRunMessage, SubAgentRunMessageRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<SubAgentRunMessage> {
    Some(SubAgentRunMessage {
        id: doc.get_str("id").ok()?.to_string(),
        run_id: doc.get_str("run_id").ok()?.to_string(),
        role: doc.get_str("role").ok()?.to_string(),
        content: doc.get_str("content").ok()?.to_string(),
        tool_call_id: doc
            .get_str("tool_call_id")
            .ok()
            .map(|value| value.to_string()),
        reasoning: doc.get_str("reasoning").ok().map(|value| value.to_string()),
        metadata: doc
            .get_str("metadata")
            .ok()
            .and_then(|raw| serde_json::from_str(raw).ok()),
        summary_status: doc
            .get_str("summary_status")
            .ok()
            .map(|value| value.to_string()),
        summary_id: doc
            .get_str("summary_id")
            .ok()
            .map(|value| value.to_string()),
        summarized_at: doc
            .get_str("summarized_at")
            .ok()
            .map(|value| value.to_string()),
        created_at: doc.get_str("created_at").ok()?.to_string(),
    })
}

pub async fn create_message(message: &SubAgentRunMessage) -> Result<SubAgentRunMessage, String> {
    let data_mongo = message.clone();
    let data_sqlite = message.clone();
    let metadata_str = message.metadata.as_ref().map(|value| value.to_string());
    let metadata_mongo = metadata_str.clone();
    let metadata_sqlite = metadata_str.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(data_mongo.id.clone())),
                ("run_id", Bson::String(data_mongo.run_id.clone())),
                ("role", Bson::String(data_mongo.role.clone())),
                ("content", Bson::String(data_mongo.content.clone())),
                (
                    "tool_call_id",
                    crate::core::values::optional_string_bson(data_mongo.tool_call_id.clone()),
                ),
                (
                    "reasoning",
                    crate::core::values::optional_string_bson(data_mongo.reasoning.clone()),
                ),
                (
                    "metadata",
                    crate::core::values::optional_string_bson(metadata_mongo.clone()),
                ),
                (
                    "summary_status",
                    crate::core::values::optional_string_bson(data_mongo.summary_status.clone()),
                ),
                (
                    "summary_id",
                    crate::core::values::optional_string_bson(data_mongo.summary_id.clone()),
                ),
                (
                    "summarized_at",
                    crate::core::values::optional_string_bson(data_mongo.summarized_at.clone()),
                ),
                ("created_at", Bson::String(data_mongo.created_at.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("sub_agent_run_messages")
                    .insert_one(doc, None)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(data_mongo.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO sub_agent_run_messages (id, run_id, role, content, tool_call_id, reasoning, metadata, summary_status, summary_id, summarized_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&data_sqlite.id)
                    .bind(&data_sqlite.run_id)
                    .bind(&data_sqlite.role)
                    .bind(&data_sqlite.content)
                    .bind(&data_sqlite.tool_call_id)
                    .bind(&data_sqlite.reasoning)
                    .bind(metadata_sqlite.as_deref())
                    .bind(&data_sqlite.summary_status)
                    .bind(&data_sqlite.summary_id)
                    .bind(&data_sqlite.summarized_at)
                    .bind(&data_sqlite.created_at)
                    .execute(pool)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(data_sqlite.clone())
            })
        },
    )
    .await
}

pub async fn list_messages_by_run(
    run_id: &str,
    limit: Option<i64>,
) -> Result<Vec<SubAgentRunMessage>, String> {
    with_db(
        |db| {
            let run_id = run_id.to_string();
            Box::pin(async move {
                let mut options = mongodb::options::FindOptions::default();
                options.sort = Some(doc! { "created_at": 1 });
                if let Some(value) = limit {
                    if value > 0 {
                        options.limit = Some(value);
                    }
                }
                let cursor = db
                    .collection::<Document>("sub_agent_run_messages")
                    .find(doc! { "run_id": run_id }, options)
                    .await
                    .map_err(|err| err.to_string())?;
                collect_map_sorted_asc(
                    cursor,
                    normalize_from_doc,
                    |message| message.created_at.as_str(),
                )
                .await
            })
        },
        |pool| {
            let run_id = run_id.to_string();
            Box::pin(async move {
                let rows = if let Some(value) = limit {
                    if value > 0 {
                        sqlx::query_as::<_, SubAgentRunMessageRow>(
                            "SELECT * FROM sub_agent_run_messages WHERE run_id = ? ORDER BY created_at ASC LIMIT ?",
                        )
                        .bind(&run_id)
                        .bind(value)
                        .fetch_all(pool)
                        .await
                        .map_err(|err| err.to_string())?
                    } else {
                        sqlx::query_as::<_, SubAgentRunMessageRow>(
                            "SELECT * FROM sub_agent_run_messages WHERE run_id = ? ORDER BY created_at ASC",
                        )
                        .bind(&run_id)
                        .fetch_all(pool)
                        .await
                        .map_err(|err| err.to_string())?
                    }
                } else {
                    sqlx::query_as::<_, SubAgentRunMessageRow>(
                        "SELECT * FROM sub_agent_run_messages WHERE run_id = ? ORDER BY created_at ASC",
                    )
                    .bind(&run_id)
                    .fetch_all(pool)
                    .await
                    .map_err(|err| err.to_string())?
                };

                Ok(rows.into_iter().map(|row| row.to_message()).collect())
            })
        },
    )
    .await
}

pub async fn list_runs_with_pending_summary(limit: Option<i64>) -> Result<Vec<String>, String> {
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
                    .collection::<Document>("sub_agent_run_messages")
                    .find(filter, None)
                    .await
                    .map_err(|err| err.to_string())?;
                let items = collect_map_sorted_asc(cursor, normalize_from_doc, |message| {
                    message.created_at.as_str()
                })
                .await?;

                let mut out: Vec<String> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                for item in items {
                    if item.run_id.trim().is_empty() {
                        continue;
                    }
                    if !seen.insert(item.run_id.clone()) {
                        continue;
                    }
                    out.push(item.run_id);
                    if let Some(max) = limit {
                        if max > 0 && out.len() as i64 >= max {
                            break;
                        }
                    }
                }
                Ok(out)
            })
        },
        |pool| {
            Box::pin(async move {
                let query = if let Some(value) = limit {
                    if value > 0 {
                        "SELECT run_id FROM sub_agent_run_messages WHERE summary_status = 'pending' OR summary_status IS NULL GROUP BY run_id ORDER BY MIN(created_at) ASC LIMIT ?".to_string()
                    } else {
                        "SELECT run_id FROM sub_agent_run_messages WHERE summary_status = 'pending' OR summary_status IS NULL GROUP BY run_id ORDER BY MIN(created_at) ASC".to_string()
                    }
                } else {
                    "SELECT run_id FROM sub_agent_run_messages WHERE summary_status = 'pending' OR summary_status IS NULL GROUP BY run_id ORDER BY MIN(created_at) ASC".to_string()
                };
                let mut q = sqlx::query(&query);
                if let Some(value) = limit {
                    if value > 0 {
                        q = q.bind(value);
                    }
                }
                let rows = q.fetch_all(pool).await.map_err(|err| err.to_string())?;
                Ok(rows
                    .into_iter()
                    .filter_map(|row| row.try_get::<String, _>("run_id").ok())
                    .collect())
            })
        },
    )
    .await
}

pub async fn get_pending_messages_for_summary(
    run_id: &str,
    limit: Option<i64>,
) -> Result<Vec<SubAgentRunMessage>, String> {
    with_db(
        |db| {
            let run_id = run_id.to_string();
            Box::pin(async move {
                let mut options = mongodb::options::FindOptions::default();
                options.sort = Some(doc! { "created_at": 1 });
                if let Some(l) = limit {
                    if l > 0 {
                        options.limit = Some(l);
                    }
                }
                let cursor = db
                    .collection::<Document>("sub_agent_run_messages")
                    .find(
                        doc! {
                            "run_id": run_id,
                            "$or": [
                                { "summary_status": "pending" },
                                { "summary_status": { "$exists": false } },
                                { "summary_status": Bson::Null }
                            ]
                        },
                        options,
                    )
                    .await
                    .map_err(|err| err.to_string())?;
                collect_map_sorted_asc(cursor, normalize_from_doc, |message| {
                    message.created_at.as_str()
                })
                .await
            })
        },
        |pool| {
            let run_id = run_id.to_string();
            Box::pin(async move {
                let rows = if let Some(l) = limit {
                    if l > 0 {
                        sqlx::query_as::<_, SubAgentRunMessageRow>("SELECT * FROM sub_agent_run_messages WHERE run_id = ? AND (summary_status = 'pending' OR summary_status IS NULL) ORDER BY created_at ASC LIMIT ?")
                            .bind(&run_id)
                            .bind(l)
                            .fetch_all(pool)
                            .await
                            .map_err(|err| err.to_string())?
                    } else {
                        sqlx::query_as::<_, SubAgentRunMessageRow>("SELECT * FROM sub_agent_run_messages WHERE run_id = ? AND (summary_status = 'pending' OR summary_status IS NULL) ORDER BY created_at ASC")
                            .bind(&run_id)
                            .fetch_all(pool)
                            .await
                            .map_err(|err| err.to_string())?
                    }
                } else {
                    sqlx::query_as::<_, SubAgentRunMessageRow>("SELECT * FROM sub_agent_run_messages WHERE run_id = ? AND (summary_status = 'pending' OR summary_status IS NULL) ORDER BY created_at ASC")
                        .bind(&run_id)
                        .fetch_all(pool)
                        .await
                        .map_err(|err| err.to_string())?
                };

                Ok(rows.into_iter().map(|row| row.to_message()).collect())
            })
        },
    )
    .await
}

pub async fn mark_messages_summarized(
    run_id: &str,
    message_ids: &[String],
    summary_id: &str,
    summarized_at: &str,
) -> Result<usize, String> {
    if message_ids.is_empty() {
        return Ok(0);
    }

    with_db(
        |db| {
            let run_id = run_id.to_string();
            let summary_id = summary_id.to_string();
            let summarized_at = summarized_at.to_string();
            let ids: Vec<Bson> = message_ids.iter().map(|id| Bson::String(id.clone())).collect();
            Box::pin(async move {
                let result = db
                    .collection::<Document>("sub_agent_run_messages")
                    .update_many(
                        doc! {
                            "run_id": run_id,
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
                    .map_err(|err| err.to_string())?;
                Ok(result.modified_count as usize)
            })
        },
        |pool| {
            let run_id = run_id.to_string();
            let summary_id = summary_id.to_string();
            let summarized_at = summarized_at.to_string();
            let ids = message_ids.to_vec();
            Box::pin(async move {
                let placeholders = vec!["?"; ids.len()].join(", ");
                let query = format!(
                    "UPDATE sub_agent_run_messages SET summary_status = 'summarized', summary_id = ?, summarized_at = ? WHERE run_id = ? AND id IN ({})",
                    placeholders
                );
                let mut q = sqlx::query(&query)
                    .bind(&summary_id)
                    .bind(&summarized_at)
                    .bind(&run_id);
                for id in ids {
                    q = q.bind(id);
                }
                let result = q.execute(pool).await.map_err(|err| err.to_string())?;
                Ok(result.rows_affected() as usize)
            })
        },
    )
    .await
}
