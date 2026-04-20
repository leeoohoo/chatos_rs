use crate::{
    domain::{datasource::DataSource, metadata::ObjectStatsResponse},
    error::{AppError, AppResult},
};

use super::super::connection::connect_pool;

pub async fn object_stats(
    datasource: &DataSource,
    database: &str,
) -> AppResult<ObjectStatsResponse> {
    if database != "main" {
        return Err(AppError::NotFound(format!(
            "sqlite database {database} not found (expected main)"
        )));
    }

    let pool = connect_pool(datasource).await?;

    let table_count = count_query(
        &pool,
        "select count(*) from sqlite_master where type = 'table' and name not like 'sqlite_%'",
    )
    .await?;
    let view_count = count_query(
        &pool,
        "select count(*) from sqlite_master where type = 'view' and name not like 'sqlite_%'",
    )
    .await?;
    let index_count = count_query(
        &pool,
        "select count(*) from sqlite_master where type = 'index' and name not like 'sqlite_%'",
    )
    .await?;
    let trigger_count = count_query(
        &pool,
        "select count(*) from sqlite_master where type = 'trigger' and name not like 'sqlite_%'",
    )
    .await?;

    Ok(ObjectStatsResponse {
        database: database.to_string(),
        schema_count: Some(1),
        table_count: Some(table_count),
        view_count: Some(view_count),
        materialized_view_count: None,
        collection_count: None,
        index_count: Some(index_count),
        procedure_count: None,
        function_count: None,
        trigger_count: Some(trigger_count),
        sequence_count: None,
        synonym_count: None,
        package_count: None,
        partial: false,
    })
}

async fn count_query(pool: &sqlx::SqlitePool, sql: &str) -> AppResult<u64> {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(pool)
        .await
        .map(|value| value as u64)
        .map_err(|err| AppError::BadRequest(format!("failed to count sqlite metadata: {err}")))
}
