use std::time::Instant;

use crate::{
    domain::{
        datasource::DataSource,
        query::{QueryColumn, QueryExecuteRequest, QueryExecuteResponse},
    },
    error::{AppError, AppResult},
};
use serde_json::Value;
use sqlx::Row;

use super::connection::connect_pool;

pub async fn execute(
    datasource: &DataSource,
    request: &QueryExecuteRequest,
) -> AppResult<QueryExecuteResponse> {
    if request.sql.trim().is_empty() {
        return Err(AppError::BadRequest("sql cannot be empty".to_string()));
    }

    let database = request.database.as_deref();
    let pool = connect_pool(datasource, database).await?;
    let start = Instant::now();

    if is_read_like_sql(&request.sql) {
        execute_read_query(&pool, request, start).await
    } else {
        execute_write_query(&pool, request, start).await
    }
}

async fn execute_read_query(
    pool: &sqlx::PgPool,
    request: &QueryExecuteRequest,
    start: Instant,
) -> AppResult<QueryExecuteResponse> {
    let max_rows = request.max_rows.unwrap_or(1_000).clamp(1, 10_000);
    let cleaned_sql = request.sql.trim().trim_end_matches(';');
    let wrapped_sql = format!("select to_jsonb(t) as row_json from ({cleaned_sql}) t limit $1");

    let rows = sqlx::query(&wrapped_sql)
        .bind(max_rows as i64)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to execute read query: {err}")))?;

    let row_objects = rows
        .into_iter()
        .filter_map(|row| row.try_get::<Option<Value>, _>("row_json").ok().flatten())
        .collect::<Vec<_>>();

    let columns = build_columns(&row_objects);
    let rows = row_objects
        .iter()
        .map(|row| {
            if let Value::Object(object) = row {
                columns
                    .iter()
                    .map(|column| object.get(&column.name).cloned().unwrap_or(Value::Null))
                    .collect()
            } else {
                vec![row.clone()]
            }
        })
        .collect::<Vec<Vec<Value>>>();

    Ok(QueryExecuteResponse {
        query_id: format!("q_{}", uuid::Uuid::new_v4().simple()),
        columns,
        row_count: rows.len() as u64,
        rows,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

async fn execute_write_query(
    pool: &sqlx::PgPool,
    request: &QueryExecuteRequest,
    start: Instant,
) -> AppResult<QueryExecuteResponse> {
    let result = sqlx::query(&request.sql)
        .execute(pool)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to execute mutation query: {err}")))?;

    Ok(QueryExecuteResponse {
        query_id: format!("q_{}", uuid::Uuid::new_v4().simple()),
        columns: Vec::new(),
        rows: Vec::new(),
        row_count: result.rows_affected(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

fn build_columns(rows: &[Value]) -> Vec<QueryColumn> {
    let Some(Value::Object(first_row)) = rows.first() else {
        return Vec::new();
    };

    first_row
        .iter()
        .map(|(name, value)| QueryColumn {
            name: name.to_string(),
            type_name: infer_json_type(value),
        })
        .collect()
}

fn infer_json_type(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "text".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

fn is_read_like_sql(sql: &str) -> bool {
    let normalized = sql.trim_start().to_lowercase();
    ["select", "with", "show", "explain", "values"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
}
