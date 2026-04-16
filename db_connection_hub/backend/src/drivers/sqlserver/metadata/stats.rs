use crate::{
    domain::{datasource::DataSource, metadata::ObjectStatsResponse},
    error::{AppError, AppResult},
};

use super::super::connection::{connect_client, map_db_error};

pub async fn object_stats(
    datasource: &DataSource,
    database: &str,
) -> AppResult<ObjectStatsResponse> {
    let mut client = connect_client(datasource, Some(database)).await?;
    let rows = client
        .query(
            "select
                cast((select count(*) from sys.schemas where name not in ('sys', 'INFORMATION_SCHEMA')) as bigint) as schema_count,
                cast((select count(*)
                      from sys.tables t
                      join sys.schemas s on s.schema_id = t.schema_id
                      where t.is_ms_shipped = 0 and s.name not in ('sys', 'INFORMATION_SCHEMA')) as bigint) as table_count,
                cast((select count(*)
                      from sys.views v
                      join sys.schemas s on s.schema_id = v.schema_id
                      where v.is_ms_shipped = 0 and s.name not in ('sys', 'INFORMATION_SCHEMA')) as bigint) as view_count,
                cast((select count(*)
                      from sys.indexes i
                      join sys.tables t on t.object_id = i.object_id
                      join sys.schemas s on s.schema_id = t.schema_id
                      where t.is_ms_shipped = 0 and s.name not in ('sys', 'INFORMATION_SCHEMA')
                        and i.index_id > 0 and i.is_hypothetical = 0) as bigint) as index_count,
                cast((select count(*)
                      from sys.procedures p
                      join sys.schemas s on s.schema_id = p.schema_id
                      where p.is_ms_shipped = 0 and s.name not in ('sys', 'INFORMATION_SCHEMA')) as bigint) as procedure_count,
                cast((select count(*)
                      from sys.objects o
                      join sys.schemas s on s.schema_id = o.schema_id
                      where o.type in ('FN', 'IF', 'TF', 'FS', 'FT')
                        and o.is_ms_shipped = 0 and s.name not in ('sys', 'INFORMATION_SCHEMA')) as bigint) as function_count,
                cast((select count(*) from sys.triggers tr where tr.parent_class_desc = 'OBJECT_OR_COLUMN') as bigint) as trigger_count,
                cast((select count(*)
                      from sys.sequences seq
                      join sys.schemas s on s.schema_id = seq.schema_id
                      where s.name not in ('sys', 'INFORMATION_SCHEMA')) as bigint) as sequence_count,
                cast((select count(*)
                      from sys.synonyms syn
                      join sys.schemas s on s.schema_id = syn.schema_id
                      where s.name not in ('sys', 'INFORMATION_SCHEMA')) as bigint) as synonym_count",
            &[],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let row = rows.first().ok_or_else(|| {
        AppError::BadRequest("failed to read sql server object stats".to_string())
    })?;

    Ok(ObjectStatsResponse {
        database: database.to_string(),
        schema_count: Some(to_u64(row.get::<i64, _>(0))),
        table_count: Some(to_u64(row.get::<i64, _>(1))),
        view_count: Some(to_u64(row.get::<i64, _>(2))),
        materialized_view_count: None,
        collection_count: None,
        index_count: Some(to_u64(row.get::<i64, _>(3))),
        procedure_count: Some(to_u64(row.get::<i64, _>(4))),
        function_count: Some(to_u64(row.get::<i64, _>(5))),
        trigger_count: Some(to_u64(row.get::<i64, _>(6))),
        sequence_count: Some(to_u64(row.get::<i64, _>(7))),
        synonym_count: Some(to_u64(row.get::<i64, _>(8))),
        package_count: None,
        partial: false,
    })
}

fn to_u64(value: Option<i64>) -> u64 {
    value.unwrap_or(0).max(0) as u64
}
