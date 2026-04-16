use crate::{
    domain::datasource::{DataSource, DatabaseInfo, DatabaseListResponse, DatabaseSummaryResponse},
    error::{AppError, AppResult},
};
use sqlx::Row;

use super::{super::connection::connect_pool, common::scoped_database};

const NON_SYSTEM_FILTER: &str =
    "schema_name not in ('information_schema','mysql','performance_schema','sys')";

pub async fn database_summary(datasource: &DataSource) -> AppResult<DatabaseSummaryResponse> {
    let pool = connect_pool(datasource, None).await?;
    let total = sqlx::query_scalar::<_, i64>(&format!(
        "select count(*) from information_schema.schemata where {NON_SYSTEM_FILTER}"
    ))
    .fetch_one(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query database summary: {err}")))?;

    if let Some(scoped) = scoped_database(datasource) {
        let visible = sqlx::query_scalar::<_, i64>(&format!(
            "select count(*) from information_schema.schemata where {NON_SYSTEM_FILTER} and schema_name = ?"
        ))
        .bind(scoped)
        .fetch_one(&pool)
        .await
        .map_err(|err| {
            AppError::BadRequest(format!("failed to query scoped database summary: {err}"))
        })?;

        Ok(DatabaseSummaryResponse {
            database_count: total as u64,
            visible_database_count: visible as u64,
            visibility_scope: "single".to_string(),
        })
    } else {
        Ok(DatabaseSummaryResponse {
            database_count: total as u64,
            visible_database_count: total as u64,
            visibility_scope: "full".to_string(),
        })
    }
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
    let scope = scoped_database(datasource);

    let rows = match (scope, keyword) {
        (Some(scoped), Some(keyword)) => {
            sqlx::query(&format!(
                "select schema_name as name
                 from information_schema.schemata
                 where {NON_SYSTEM_FILTER}
                   and schema_name = ?
                   and schema_name like ?
                 order by schema_name
                 limit ? offset ?"
            ))
            .bind(scoped)
            .bind(format!("%{keyword}%"))
            .bind(safe_size as i64)
            .bind(offset)
            .fetch_all(&pool)
            .await
        }
        (Some(scoped), None) => {
            sqlx::query(&format!(
                "select schema_name as name
                 from information_schema.schemata
                 where {NON_SYSTEM_FILTER}
                   and schema_name = ?
                 order by schema_name
                 limit ? offset ?"
            ))
            .bind(scoped)
            .bind(safe_size as i64)
            .bind(offset)
            .fetch_all(&pool)
            .await
        }
        (None, Some(keyword)) => {
            sqlx::query(&format!(
                "select schema_name as name
                 from information_schema.schemata
                 where {NON_SYSTEM_FILTER}
                   and schema_name like ?
                 order by schema_name
                 limit ? offset ?"
            ))
            .bind(format!("%{keyword}%"))
            .bind(safe_size as i64)
            .bind(offset)
            .fetch_all(&pool)
            .await
        }
        (None, None) => {
            sqlx::query(&format!(
                "select schema_name as name
                 from information_schema.schemata
                 where {NON_SYSTEM_FILTER}
                 order by schema_name
                 limit ? offset ?"
            ))
            .bind(safe_size as i64)
            .bind(offset)
            .fetch_all(&pool)
            .await
        }
    }
    .map_err(|err| AppError::BadRequest(format!("failed to list databases: {err}")))?;

    let total = match (scope, keyword) {
        (Some(scoped), Some(keyword)) => {
            sqlx::query_scalar::<_, i64>(&format!(
                "select count(*) from information_schema.schemata
                 where {NON_SYSTEM_FILTER}
                   and schema_name = ?
                   and schema_name like ?"
            ))
            .bind(scoped)
            .bind(format!("%{keyword}%"))
            .fetch_one(&pool)
            .await
        }
        (Some(scoped), None) => {
            sqlx::query_scalar::<_, i64>(&format!(
                "select count(*) from information_schema.schemata
                 where {NON_SYSTEM_FILTER}
                   and schema_name = ?"
            ))
            .bind(scoped)
            .fetch_one(&pool)
            .await
        }
        (None, Some(keyword)) => {
            sqlx::query_scalar::<_, i64>(&format!(
                "select count(*) from information_schema.schemata
                 where {NON_SYSTEM_FILTER}
                   and schema_name like ?"
            ))
            .bind(format!("%{keyword}%"))
            .fetch_one(&pool)
            .await
        }
        (None, None) => {
            sqlx::query_scalar::<_, i64>(&format!(
                "select count(*) from information_schema.schemata where {NON_SYSTEM_FILTER}"
            ))
            .fetch_one(&pool)
            .await
        }
    }
    .map_err(|err| AppError::BadRequest(format!("failed to count databases: {err}")))?;

    let items = rows
        .into_iter()
        .map(|row| DatabaseInfo {
            name: row.try_get::<String, _>("name").unwrap_or_default(),
            owner: None,
            size_bytes: None,
        })
        .collect();

    Ok(DatabaseListResponse {
        items,
        page: safe_page,
        page_size: safe_size,
        total: total as u64,
    })
}
