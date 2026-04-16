use crate::{
    domain::{datasource::DataSource, metadata::ObjectStatsResponse},
    error::{AppError, AppResult},
};

use super::{super::connection::connect_pool, common::ensure_database_in_scope};

pub async fn object_stats(
    datasource: &DataSource,
    database: &str,
) -> AppResult<ObjectStatsResponse> {
    ensure_database_in_scope(datasource, database)?;
    let pool = connect_pool(datasource, Some(database)).await?;

    let table_count = count_query(
        &pool,
        "select count(*) from information_schema.tables where table_schema = ? and table_type = 'BASE TABLE'",
        database,
    )
    .await?;
    let view_count = count_query(
        &pool,
        "select count(*) from information_schema.views where table_schema = ?",
        database,
    )
    .await?;
    let index_count = count_query(
        &pool,
        "select count(*) from information_schema.statistics where table_schema = ?",
        database,
    )
    .await?;
    let procedure_count = count_query(
        &pool,
        "select count(*) from information_schema.routines where routine_schema = ? and routine_type = 'PROCEDURE'",
        database,
    )
    .await?;
    let function_count = count_query(
        &pool,
        "select count(*) from information_schema.routines where routine_schema = ? and routine_type = 'FUNCTION'",
        database,
    )
    .await?;
    let trigger_count = count_query(
        &pool,
        "select count(*) from information_schema.triggers where trigger_schema = ?",
        database,
    )
    .await?;

    Ok(ObjectStatsResponse {
        database: database.to_string(),
        schema_count: None,
        table_count: Some(table_count),
        view_count: Some(view_count),
        materialized_view_count: None,
        collection_count: None,
        index_count: Some(index_count),
        procedure_count: Some(procedure_count),
        function_count: Some(function_count),
        trigger_count: Some(trigger_count),
        sequence_count: None,
        synonym_count: None,
        package_count: None,
        partial: false,
    })
}

async fn count_query(pool: &sqlx::MySqlPool, sql: &str, database: &str) -> AppResult<u64> {
    sqlx::query_scalar::<_, i64>(sql)
        .bind(database)
        .fetch_one(pool)
        .await
        .map(|value| value as u64)
        .map_err(|err| AppError::BadRequest(format!("failed to count metadata: {err}")))
}
