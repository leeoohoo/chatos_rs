use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::JobRun;

use super::now_rfc3339;

fn collection(db: &Db) -> mongodb::Collection<JobRun> {
    db.collection::<JobRun>("job_runs")
}

fn doc_i64(doc: &Document, key: &str) -> i64 {
    match doc.get(key) {
        Some(Bson::Int32(v)) => *v as i64,
        Some(Bson::Int64(v)) => *v,
        Some(Bson::Double(v)) => *v as i64,
        _ => 0,
    }
}

pub async fn create_job_run(
    db: &Db,
    job_type: &str,
    session_id: Option<&str>,
    trigger_type: Option<&str>,
    input_count: i64,
) -> Result<JobRun, String> {
    let job = JobRun {
        id: Uuid::new_v4().to_string(),
        job_type: job_type.to_string(),
        session_id: session_id.map(|v| v.to_string()),
        status: "running".to_string(),
        trigger_type: trigger_type.map(|v| v.to_string()),
        input_count,
        output_count: 0,
        error_message: None,
        started_at: now_rfc3339(),
        finished_at: None,
    };

    collection(db)
        .insert_one(job.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(job)
}

pub async fn finish_job_run(
    db: &Db,
    job_run_id: &str,
    status: &str,
    output_count: i64,
    error_message: Option<&str>,
) -> Result<(), String> {
    collection(db)
        .update_one(
            doc! {"id": job_run_id},
            doc! {
                "$set": {
                    "status": status,
                    "output_count": output_count,
                    "error_message": error_message,
                    "finished_at": now_rfc3339(),
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn list_job_runs(
    db: &Db,
    job_type: Option<&str>,
    session_id: Option<&str>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<JobRun>, String> {
    let mut filter = doc! {};
    if let Some(v) = job_type {
        filter.insert("job_type", v);
    }
    if let Some(v) = session_id {
        filter.insert("session_id", v);
    }
    if let Some(v) = status {
        filter.insert("status", v);
    }

    let options = FindOptions::builder()
        .sort(doc! {"started_at": -1})
        .limit(Some(limit.max(1).min(1000)))
        .build();

    let cursor = collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn job_stats(db: &Db) -> Result<serde_json::Value, String> {
    let since = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    let pipeline = vec![
        doc! {"$match": {"started_at": {"$gte": since}}},
        doc! {"$group": {"_id": {"job_type": "$job_type", "status": "$status"}, "count": {"$sum": 1}}},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("job_runs")
        .aggregate(pipeline)
        .await
        .map_err(|e| e.to_string())?;
    let docs: Vec<mongodb::bson::Document> =
        cursor.try_collect().await.map_err(|e| e.to_string())?;

    let mut map = serde_json::Map::new();
    for doc in docs {
        let Some(id_doc) = doc.get_document("_id").ok() else {
            continue;
        };
        let Some(job_type) = id_doc.get_str("job_type").ok() else {
            continue;
        };
        let Some(status) = id_doc.get_str("status").ok() else {
            continue;
        };
        let count = doc_i64(&doc, "count");

        let entry = map
            .entry(job_type.to_string())
            .or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = entry.as_object_mut() {
            obj.insert(status.to_string(), serde_json::json!(count));
        }
    }

    Ok(serde_json::Value::Object(map))
}
