use std::time::Instant;

use crate::{
    domain::{
        datasource::DataSource,
        query::{QueryColumn, QueryExecuteRequest, QueryExecuteResponse},
    },
    error::{AppError, AppResult},
};
use serde_json::Value;
use sqlx::{Column, Row};

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
    pool: &sqlx::MySqlPool,
    request: &QueryExecuteRequest,
    start: Instant,
) -> AppResult<QueryExecuteResponse> {
    let max_rows = request.max_rows.unwrap_or(1_000).clamp(1, 10_000);
    let cleaned_sql = request.sql.trim().trim_end_matches(';');
    let rows = sqlx::query(cleaned_sql)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to execute read query: {err}")))?;

    let columns = if let Some(first_row) = rows.first() {
        first_row
            .columns()
            .iter()
            .map(|column| QueryColumn {
                name: column.name().to_string(),
                type_name: "unknown".to_string(),
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let mut result_rows: Vec<Vec<Value>> = Vec::new();
    for row in rows.into_iter().take(max_rows as usize) {
        let values = (0..columns.len())
            .map(|index| decode_cell(&row, index))
            .collect::<Vec<_>>();
        result_rows.push(values);
    }

    Ok(QueryExecuteResponse {
        query_id: format!("q_{}", uuid::Uuid::new_v4().simple()),
        columns,
        row_count: result_rows.len() as u64,
        rows: result_rows,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

async fn execute_write_query(
    pool: &sqlx::MySqlPool,
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

fn decode_cell(row: &sqlx::mysql::MySqlRow, index: usize) -> Value {
    if let Ok(value) = row.try_get::<Option<String>, _>(index) {
        return value.map(Value::String).unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Option<u64>, _>(index) {
        return value.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Option<f64>, _>(index) {
        return value.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Option<bool>, _>(index) {
        return value.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Option<Vec<u8>>, _>(index) {
        return value
            .map(|bytes| Value::String(String::from_utf8_lossy(&bytes).to_string()))
            .unwrap_or(Value::Null);
    }
    Value::Null
}

fn is_read_like_sql(sql: &str) -> bool {
    let normalized = sql.trim_start().to_lowercase();
    [
        "select", "with", "show", "explain", "values", "desc", "describe",
    ]
    .iter()
    .any(|prefix| normalized.starts_with(prefix))
}
