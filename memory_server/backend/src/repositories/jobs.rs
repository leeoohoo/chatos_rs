use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::models::JobRun;

use super::now_rfc3339;

pub async fn create_job_run(
    pool: &SqlitePool,
    job_type: &str,
    session_id: Option<&str>,
    trigger_type: Option<&str>,
    input_count: i64,
) -> Result<JobRun, String> {
    let id = Uuid::new_v4().to_string();
    let now = now_rfc3339();
    sqlx::query("INSERT INTO job_runs (id, job_type, session_id, status, trigger_type, input_count, output_count, error_message, started_at, finished_at) VALUES (?, ?, ?, 'running', ?, ?, 0, NULL, ?, NULL)")
        .bind(&id)
        .bind(job_type)
        .bind(session_id)
        .bind(trigger_type)
        .bind(input_count)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    get_job_run_by_id(pool, &id)
        .await?
        .ok_or_else(|| "created job run not found".to_string())
}

pub async fn finish_job_run(
    pool: &SqlitePool,
    job_run_id: &str,
    status: &str,
    output_count: i64,
    error_message: Option<&str>,
) -> Result<(), String> {
    let now = now_rfc3339();
    sqlx::query("UPDATE job_runs SET status = ?, output_count = ?, error_message = ?, finished_at = ? WHERE id = ?")
        .bind(status)
        .bind(output_count)
        .bind(error_message)
        .bind(now)
        .bind(job_run_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_job_run_by_id(pool: &SqlitePool, id: &str) -> Result<Option<JobRun>, String> {
    sqlx::query_as::<_, JobRun>("SELECT * FROM job_runs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_job_runs(
    pool: &SqlitePool,
    job_type: Option<&str>,
    session_id: Option<&str>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<JobRun>, String> {
    let mut sql = "SELECT * FROM job_runs WHERE 1=1".to_string();
    if job_type.is_some() {
        sql.push_str(" AND job_type = ?");
    }
    if session_id.is_some() {
        sql.push_str(" AND session_id = ?");
    }
    if status.is_some() {
        sql.push_str(" AND status = ?");
    }
    sql.push_str(" ORDER BY started_at DESC LIMIT ?");

    let mut q = sqlx::query_as::<_, JobRun>(&sql);
    if let Some(v) = job_type {
        q = q.bind(v);
    }
    if let Some(v) = session_id {
        q = q.bind(v);
    }
    if let Some(v) = status {
        q = q.bind(v);
    }

    q.bind(limit.max(1).min(1000))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn job_stats(pool: &SqlitePool) -> Result<serde_json::Value, String> {
    let rows = sqlx::query(
        "SELECT job_type, status, COUNT(*) as count FROM job_runs WHERE started_at >= datetime('now', '-1 day') GROUP BY job_type, status",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut map = serde_json::Map::new();
    for row in rows {
        let job_type: String = row.try_get("job_type").unwrap_or_default();
        let status: String = row.try_get("status").unwrap_or_default();
        let count: i64 = row.try_get("count").unwrap_or(0);

        let entry = map
            .entry(job_type)
            .or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = entry.as_object_mut() {
            obj.insert(status, serde_json::json!(count));
        }
    }

    Ok(serde_json::Value::Object(map))
}
