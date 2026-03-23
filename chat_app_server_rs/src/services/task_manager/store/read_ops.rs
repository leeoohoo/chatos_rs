use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use sqlx::{QueryBuilder, Sqlite};

use crate::repositories::db::with_db;
use crate::services::task_manager::mapper::task_record_from_doc;
use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::TaskRecord;

use super::row::TaskRow;

pub async fn list_tasks_for_context(
    session_id: &str,
    conversation_turn_id: Option<&str>,
    include_done: bool,
    limit: usize,
) -> Result<Vec<TaskRecord>, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let conversation_turn_id = conversation_turn_id
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let limit = limit.clamp(1, 200) as i64;
    let session_id_for_mongo = session_id.clone();
    let conversation_turn_id_for_mongo = conversation_turn_id.clone();
    let session_id_for_sqlite = session_id.clone();
    let conversation_turn_id_for_sqlite = conversation_turn_id.clone();

    with_db(
        move |db| {
            let session_id = session_id_for_mongo.clone();
            let conversation_turn_id = conversation_turn_id_for_mongo.clone();
            Box::pin(async move {
                let mut filter = doc! { "session_id": session_id };
                if let Some(turn_id) = conversation_turn_id {
                    filter.insert("conversation_turn_id", Bson::String(turn_id));
                }
                if !include_done {
                    filter.insert("status", doc! { "$ne": "done" });
                }

                let find_options = FindOptions::builder()
                    .sort(doc! { "created_at": -1 })
                    .limit(limit)
                    .build();
                let mut cursor = db
                    .collection::<Document>("task_manager_tasks")
                    .find(filter, find_options)
                    .await
                    .map_err(|err| err.to_string())?;

                let mut out = Vec::new();
                while cursor.advance().await.map_err(|err| err.to_string())? {
                    let document = cursor.deserialize_current().map_err(|err| err.to_string())?;
                    if let Some(task) = task_record_from_doc(&document) {
                        out.push(task);
                    }
                }
                Ok(out)
            })
        },
        move |pool| {
            let session_id = session_id_for_sqlite.clone();
            let conversation_turn_id = conversation_turn_id_for_sqlite.clone();
            Box::pin(async move {
                let mut qb = QueryBuilder::<Sqlite>::new(
                    "SELECT id, session_id, conversation_turn_id, title, details, priority, status, tags_json, due_at, created_at, updated_at FROM task_manager_tasks WHERE session_id = ",
                );
                qb.push_bind(session_id);
                if let Some(turn_id) = conversation_turn_id {
                    qb.push(" AND conversation_turn_id = ");
                    qb.push_bind(turn_id);
                }
                if !include_done {
                    qb.push(" AND status != ");
                    qb.push_bind("done");
                }
                qb.push(" ORDER BY created_at DESC LIMIT ");
                qb.push_bind(limit);

                let rows: Vec<TaskRow> = qb
                    .build_query_as()
                    .fetch_all(pool)
                    .await
                    .map_err(|err| err.to_string())?;

                Ok(rows.into_iter().map(TaskRow::into_record).collect())
            })
        },
    )
    .await
}
