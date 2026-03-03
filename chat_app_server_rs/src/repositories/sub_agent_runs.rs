use mongodb::bson::{doc, Bson, Document};

use crate::models::sub_agent_run::{SubAgentRun, SubAgentRunRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<SubAgentRun> {
    Some(SubAgentRun {
        id: doc.get_str("id").ok()?.to_string(),
        status: doc.get_str("status").ok()?.to_string(),
        task: doc.get_str("task").ok()?.to_string(),
        agent_id: doc.get_str("agent_id").ok().map(|value| value.to_string()),
        command_id: doc
            .get_str("command_id")
            .ok()
            .map(|value| value.to_string()),
        payload_json: doc
            .get_str("payload_json")
            .ok()
            .map(|value| value.to_string()),
        result_json: doc
            .get_str("result_json")
            .ok()
            .map(|value| value.to_string()),
        error: doc.get_str("error").ok().map(|value| value.to_string()),
        created_at: doc.get_str("created_at").ok()?.to_string(),
        updated_at: doc.get_str("updated_at").ok()?.to_string(),
        session_id: doc.get_str("session_id").ok()?.to_string(),
        run_id: doc.get_str("run_id").ok()?.to_string(),
    })
}

pub async fn create_run(run: &SubAgentRun) -> Result<SubAgentRun, String> {
    let data_mongo = run.clone();
    let data_sqlite = run.clone();
    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(data_mongo.id.clone())),
                ("status", Bson::String(data_mongo.status.clone())),
                ("task", Bson::String(data_mongo.task.clone())),
                (
                    "agent_id",
                    crate::core::values::optional_string_bson(data_mongo.agent_id.clone()),
                ),
                (
                    "command_id",
                    crate::core::values::optional_string_bson(data_mongo.command_id.clone()),
                ),
                (
                    "payload_json",
                    crate::core::values::optional_string_bson(data_mongo.payload_json.clone()),
                ),
                (
                    "result_json",
                    crate::core::values::optional_string_bson(data_mongo.result_json.clone()),
                ),
                (
                    "error",
                    crate::core::values::optional_string_bson(data_mongo.error.clone()),
                ),
                ("created_at", Bson::String(data_mongo.created_at.clone())),
                ("updated_at", Bson::String(data_mongo.updated_at.clone())),
                ("session_id", Bson::String(data_mongo.session_id.clone())),
                ("run_id", Bson::String(data_mongo.run_id.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("sub_agent_runs")
                    .insert_one(doc, None)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(data_mongo.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO sub_agent_runs (id, status, task, agent_id, command_id, payload_json, result_json, error, created_at, updated_at, session_id, run_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&data_sqlite.id)
                    .bind(&data_sqlite.status)
                    .bind(&data_sqlite.task)
                    .bind(&data_sqlite.agent_id)
                    .bind(&data_sqlite.command_id)
                    .bind(&data_sqlite.payload_json)
                    .bind(&data_sqlite.result_json)
                    .bind(&data_sqlite.error)
                    .bind(&data_sqlite.created_at)
                    .bind(&data_sqlite.updated_at)
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

pub async fn get_run_by_id(id: &str) -> Result<Option<SubAgentRun>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("sub_agent_runs")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(doc.and_then(|value| normalize_from_doc(&value)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, SubAgentRunRow>(
                    "SELECT * FROM sub_agent_runs WHERE id = ? LIMIT 1",
                )
                .bind(&id)
                .fetch_optional(pool)
                .await
                .map_err(|err| err.to_string())?;
                Ok(row.map(|value| value.to_run()))
            })
        },
    )
    .await
}

pub async fn update_run_status(
    id: &str,
    status: &str,
    result_json: Option<String>,
    error: Option<String>,
) -> Result<Option<SubAgentRun>, String> {
    let now = crate::core::time::now_rfc3339();
    let id_mongo = id.to_string();
    let id_sqlite = id.to_string();
    let status_mongo = status.to_string();
    let status_sqlite = status.to_string();
    let result_mongo = result_json.clone();
    let result_sqlite = result_json.clone();
    let error_mongo = error.clone();
    let error_sqlite = error.clone();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();

    with_db(
        |db| {
            Box::pin(async move {
                db.collection::<Document>("sub_agent_runs")
                    .update_one(
                        doc! { "id": id_mongo.clone() },
                        doc! {
                            "$set": {
                                "status": status_mongo,
                                "result_json": result_mongo,
                                "error": error_mongo,
                                "updated_at": now_mongo,
                            }
                        },
                        None,
                    )
                    .await
                    .map_err(|err| err.to_string())?;
                let doc = db
                    .collection::<Document>("sub_agent_runs")
                    .find_one(doc! { "id": id_mongo }, None)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(doc.and_then(|value| normalize_from_doc(&value)))
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("UPDATE sub_agent_runs SET status = ?, result_json = ?, error = ?, updated_at = ? WHERE id = ?")
                    .bind(&status_sqlite)
                    .bind(&result_sqlite)
                    .bind(&error_sqlite)
                    .bind(&now_sqlite)
                    .bind(&id_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|err| err.to_string())?;
                let row = sqlx::query_as::<_, SubAgentRunRow>(
                    "SELECT * FROM sub_agent_runs WHERE id = ? LIMIT 1",
                )
                .bind(&id_sqlite)
                .fetch_optional(pool)
                .await
                .map_err(|err| err.to_string())?;
                Ok(row.map(|value| value.to_run()))
            })
        },
    )
    .await
}
