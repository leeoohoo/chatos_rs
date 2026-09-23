// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::{Postgres, QueryBuilder};

use crate::db::Db;

pub async fn job_run_stats(
    db: &Db,
    job_type: Option<&str>,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    since_hours: i64,
) -> Result<serde_json::Value, String> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT job_type,status,COUNT(*) FROM engine_job_runs WHERE started_at>=",
    );
    query.push_bind(chrono::Utc::now() - chrono::Duration::hours(since_hours.max(1)));
    for (column, value) in [
        ("job_type", normalized(job_type)),
        ("tenant_id", normalized(tenant_id)),
        ("source_id", normalized(source_id)),
    ] {
        if let Some(value) = value {
            query.push(" AND ").push(column).push("=").push_bind(value);
        }
    }
    query.push(" GROUP BY job_type,status");
    let rows = query
        .build_query_as::<(String, String, i64)>()
        .fetch_all(db)
        .await
        .map_err(|error| error.to_string())?;
    let mut map = serde_json::Map::new();
    for (job_type, status, count) in rows {
        let entry = map.entry(job_type).or_insert_with(|| serde_json::json!({}));
        if let Some(object) = entry.as_object_mut() {
            object.insert(status, serde_json::json!(count));
        }
    }
    Ok(serde_json::Value::Object(map))
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
