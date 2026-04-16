use std::time::Instant;

use crate::{
    domain::{
        datasource::DataSource,
        query::{QueryColumn, QueryExecuteRequest, QueryExecuteResponse},
    },
    error::{AppError, AppResult},
};
use serde_json::Value;

use super::connection::{connect_client, map_db_error};

pub async fn execute(
    datasource: &DataSource,
    request: &QueryExecuteRequest,
) -> AppResult<QueryExecuteResponse> {
    if request.sql.trim().is_empty() {
        return Err(AppError::BadRequest("sql cannot be empty".to_string()));
    }

    let mut client = connect_client(datasource, request.database.as_deref()).await?;
    let start = Instant::now();

    if is_read_like_sql(&request.sql) {
        execute_read_query(&mut client, request, start).await
    } else {
        execute_write_query(&mut client, request, start).await
    }
}

async fn execute_read_query(
    client: &mut super::connection::SqlServerClient,
    request: &QueryExecuteRequest,
    start: Instant,
) -> AppResult<QueryExecuteResponse> {
    let max_rows = request.max_rows.unwrap_or(1_000).clamp(1, 10_000);
    let cleaned_sql = request.sql.trim().trim_end_matches(';');
    let wrapped_sql = format!("select top ({max_rows}) * from ({cleaned_sql}) as q");

    let rows = client
        .query(wrapped_sql.as_str(), &[])
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

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

    let result_rows = rows
        .into_iter()
        .map(|row| {
            (0..columns.len())
                .map(|index| decode_cell(&row, index))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    Ok(QueryExecuteResponse {
        query_id: format!("q_{}", uuid::Uuid::new_v4().simple()),
        columns,
        row_count: result_rows.len() as u64,
        rows: result_rows,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

async fn execute_write_query(
    client: &mut super::connection::SqlServerClient,
    request: &QueryExecuteRequest,
    start: Instant,
) -> AppResult<QueryExecuteResponse> {
    let stream = client
        .execute(request.sql.trim(), &[])
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let affected = stream.rows_affected().first().copied().unwrap_or(0);

    Ok(QueryExecuteResponse {
        query_id: format!("q_{}", uuid::Uuid::new_v4().simple()),
        columns: Vec::new(),
        rows: Vec::new(),
        row_count: affected,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

fn decode_cell(row: &tiberius::Row, index: usize) -> Value {
    if let Some(value) = row.get::<&str, _>(index) {
        return Value::String(value.to_string());
    }
    if let Some(value) = row.get::<i64, _>(index) {
        return Value::from(value);
    }
    if let Some(value) = row.get::<i32, _>(index) {
        return Value::from(value);
    }
    if let Some(value) = row.get::<f64, _>(index) {
        return Value::from(value);
    }
    if let Some(value) = row.get::<f32, _>(index) {
        return Value::from(value as f64);
    }
    if let Some(value) = row.get::<bool, _>(index) {
        return Value::from(value);
    }

    Value::Null
}

fn is_read_like_sql(sql: &str) -> bool {
    let normalized = sql.trim_start().to_lowercase();
    ["select", "with", "show", "exec", "execute"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
}
