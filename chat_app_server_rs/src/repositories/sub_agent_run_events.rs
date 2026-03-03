use mongodb::bson::{doc, Bson, Document};

use crate::core::mongo_cursor::collect_map_sorted_asc;
use crate::models::sub_agent_run_event::{SubAgentRunEvent, SubAgentRunEventRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<SubAgentRunEvent> {
    Some(SubAgentRunEvent {
        id: doc.get_str("id").ok()?.to_string(),
        job_id: doc.get_str("job_id").ok()?.to_string(),
        event_type: doc.get_str("event_type").ok()?.to_string(),
        payload_json: doc
            .get_str("payload_json")
            .ok()
            .map(|value| value.to_string()),
        created_at: doc.get_str("created_at").ok()?.to_string(),
        session_id: doc.get_str("session_id").ok()?.to_string(),
        run_id: doc.get_str("run_id").ok()?.to_string(),
    })
}

pub async fn create_event(event: &SubAgentRunEvent) -> Result<SubAgentRunEvent, String> {
    let data_mongo = event.clone();
    let data_sqlite = event.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(data_mongo.id.clone())),
                ("job_id", Bson::String(data_mongo.job_id.clone())),
                ("event_type", Bson::String(data_mongo.event_type.clone())),
                (
                    "payload_json",
                    crate::core::values::optional_string_bson(data_mongo.payload_json.clone()),
                ),
                ("created_at", Bson::String(data_mongo.created_at.clone())),
                ("session_id", Bson::String(data_mongo.session_id.clone())),
                ("run_id", Bson::String(data_mongo.run_id.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("sub_agent_run_events")
                    .insert_one(doc, None)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(data_mongo.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO sub_agent_run_events (id, job_id, event_type, payload_json, created_at, session_id, run_id) VALUES (?, ?, ?, ?, ?, ?, ?)")
                    .bind(&data_sqlite.id)
                    .bind(&data_sqlite.job_id)
                    .bind(&data_sqlite.event_type)
                    .bind(&data_sqlite.payload_json)
                    .bind(&data_sqlite.created_at)
                    .bind(&data_sqlite.session_id)
                    .bind(&data_sqlite.run_id)
                    .execute(pool)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(data_sqlite.clone())
            })
        },
    )
    .await
}

pub async fn list_events_by_job_id(job_id: &str) -> Result<Vec<SubAgentRunEvent>, String> {
    with_db(
        |db| {
            let job_id = job_id.to_string();
            Box::pin(async move {
                let cursor = db
                    .collection::<Document>("sub_agent_run_events")
                    .find(doc! { "job_id": job_id }, None)
                    .await
                    .map_err(|err| err.to_string())?;
                collect_map_sorted_asc(cursor, normalize_from_doc, |event| {
                    event.created_at.as_str()
                })
                .await
            })
        },
        |pool| {
            let job_id = job_id.to_string();
            Box::pin(async move {
                let rows = sqlx::query_as::<_, SubAgentRunEventRow>(
                    "SELECT * FROM sub_agent_run_events WHERE job_id = ? ORDER BY created_at ASC",
                )
                .bind(&job_id)
                .fetch_all(pool)
                .await
                .map_err(|err| err.to_string())?;
                Ok(rows.into_iter().map(|row| row.to_event()).collect())
            })
        },
    )
    .await
}
