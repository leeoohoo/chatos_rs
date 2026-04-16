use crate::{
    domain::{datasource::DataSource, metadata::ObjectStatsResponse},
    error::{AppError, AppResult},
};

use super::super::connection::connect_pool;

pub async fn object_stats(
    datasource: &DataSource,
    database: &str,
) -> AppResult<ObjectStatsResponse> {
    let pool = connect_pool(datasource, Some(database)).await?;

    let schema_count = count_query(
        &pool,
        "select count(*) from pg_namespace where nspname not like 'pg_%' and nspname <> 'information_schema'",
    )
    .await?;
    let table_count = count_query(
        &pool,
        "select count(*) from pg_class where relkind = 'r' and relnamespace in (select oid from pg_namespace where nspname not like 'pg_%' and nspname <> 'information_schema')",
    )
    .await?;
    let view_count = count_query(
        &pool,
        "select count(*) from pg_class where relkind = 'v' and relnamespace in (select oid from pg_namespace where nspname not like 'pg_%' and nspname <> 'information_schema')",
    )
    .await?;
    let materialized_view_count = count_query(
        &pool,
        "select count(*) from pg_class where relkind = 'm' and relnamespace in (select oid from pg_namespace where nspname not like 'pg_%' and nspname <> 'information_schema')",
    )
    .await?;
    let index_count = count_query(
        &pool,
        "select count(*) from pg_class where relkind = 'i' and relnamespace in (select oid from pg_namespace where nspname not like 'pg_%' and nspname <> 'information_schema')",
    )
    .await?;
    let function_count = count_query(
        &pool,
        "select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname not like 'pg_%' and n.nspname <> 'information_schema'",
    )
    .await?;
    let sequence_count = count_query(
        &pool,
        "select count(*) from pg_class where relkind = 'S' and relnamespace in (select oid from pg_namespace where nspname not like 'pg_%' and nspname <> 'information_schema')",
    )
    .await?;
    let trigger_count = count_query(
        &pool,
        "select count(*) from pg_trigger where not tgisinternal",
    )
    .await?;

    Ok(ObjectStatsResponse {
        database: database.to_string(),
        schema_count: Some(schema_count),
        table_count: Some(table_count),
        view_count: Some(view_count),
        materialized_view_count: Some(materialized_view_count),
        collection_count: None,
        index_count: Some(index_count),
        procedure_count: None,
        function_count: Some(function_count),
        trigger_count: Some(trigger_count),
        sequence_count: Some(sequence_count),
        synonym_count: None,
        package_count: None,
        partial: false,
    })
}

async fn count_query(pool: &sqlx::PgPool, sql: &str) -> AppResult<u64> {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(pool)
        .await
        .map(|value| value as u64)
        .map_err(|err| AppError::BadRequest(format!("failed to count metadata: {err}")))
}
