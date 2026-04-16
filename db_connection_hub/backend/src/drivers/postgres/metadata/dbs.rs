use crate::{
    domain::datasource::{DataSource, DatabaseInfo, DatabaseListResponse, DatabaseSummaryResponse},
    error::{AppError, AppResult},
};
use sqlx::Row;

use super::super::connection::connect_pool;

pub async fn database_summary(datasource: &DataSource) -> AppResult<DatabaseSummaryResponse> {
    let pool = connect_pool(datasource, None).await?;
    let count = sqlx::query_scalar::<_, i64>(
        "select count(*) from pg_database where datistemplate = false and datallowconn = true",
    )
    .fetch_one(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query database summary: {err}")))?;

    Ok(DatabaseSummaryResponse {
        database_count: count as u64,
        visible_database_count: count as u64,
        visibility_scope: "full".to_string(),
    })
}

pub async fn list_databases(
    datasource: &DataSource,
    keyword: Option<&str>,
    page: u32,
    page_size: u32,
) -> AppResult<DatabaseListResponse> {
    let pool = connect_pool(datasource, None).await?;
    let safe_page = page.max(1);
    let safe_size = page_size.clamp(1, 500);
    let offset = ((safe_page - 1) * safe_size) as i64;

    let rows = if let Some(keyword) = keyword {
        sqlx::query(
            "select datname as name, null::text as owner, null::bigint as size_bytes
             from pg_database
             where datistemplate = false and datallowconn = true and datname ilike $1
             order by datname
             limit $2 offset $3",
        )
        .bind(format!("%{keyword}%"))
        .bind(safe_size as i64)
        .bind(offset)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "select datname as name, null::text as owner, null::bigint as size_bytes
             from pg_database
             where datistemplate = false and datallowconn = true
             order by datname
             limit $1 offset $2",
        )
        .bind(safe_size as i64)
        .bind(offset)
        .fetch_all(&pool)
        .await
    }
    .map_err(|err| AppError::BadRequest(format!("failed to list databases: {err}")))?;

    let total = if let Some(keyword) = keyword {
        sqlx::query_scalar::<_, i64>(
            "select count(*) from pg_database
             where datistemplate = false and datallowconn = true and datname ilike $1",
        )
        .bind(format!("%{keyword}%"))
        .fetch_one(&pool)
        .await
    } else {
        sqlx::query_scalar::<_, i64>(
            "select count(*) from pg_database where datistemplate = false and datallowconn = true",
        )
        .fetch_one(&pool)
        .await
    }
    .map_err(|err| AppError::BadRequest(format!("failed to count databases: {err}")))?;

    let items = rows
        .into_iter()
        .map(|row| DatabaseInfo {
            name: row.try_get::<String, _>("name").unwrap_or_default(),
            owner: row.try_get::<Option<String>, _>("owner").ok().flatten(),
            size_bytes: row
                .try_get::<Option<i64>, _>("size_bytes")
                .ok()
                .flatten()
                .map(|v| v as u64),
        })
        .collect();

    Ok(DatabaseListResponse {
        items,
        page: safe_page,
        page_size: safe_size,
        total: total as u64,
    })
}
