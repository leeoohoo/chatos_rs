use std::collections::HashMap;

use crate::{
    domain::{
        datasource::DataSource,
        metadata::{
            MetadataNodeType, ObjectColumn, ObjectConstraint, ObjectDetailResponse, ObjectIndex,
        },
    },
    error::{AppError, AppResult},
};
use sqlx::Row;

use super::{
    super::connection::connect_pool,
    common::{parse_index_columns, parse_index_node, parse_relation_node, parse_trigger_node},
};

pub async fn object_detail(
    datasource: &DataSource,
    node_id: &str,
) -> AppResult<ObjectDetailResponse> {
    if let Some((node_type, database, schema, object_name)) = parse_relation_node(node_id) {
        return load_relation_detail(
            datasource,
            node_id,
            node_type,
            &database,
            &schema,
            &object_name,
        )
        .await;
    }

    if let Some((database, schema, table, index_name)) = parse_index_node(node_id) {
        return load_index_detail(datasource, node_id, &database, &schema, &table, &index_name)
            .await;
    }

    if let Some((database, schema, table, trigger_name)) = parse_trigger_node(node_id) {
        return load_trigger_detail(
            datasource,
            node_id,
            &database,
            &schema,
            &table,
            &trigger_name,
        )
        .await;
    }

    Err(AppError::NotFound(format!(
        "unsupported node for detail: {node_id}"
    )))
}

async fn load_relation_detail(
    datasource: &DataSource,
    node_id: &str,
    node_type: MetadataNodeType,
    database: &str,
    schema: &str,
    object_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let pool = connect_pool(datasource, Some(database)).await?;

    let columns_rows = sqlx::query(
        "select column_name, data_type, is_nullable
         from information_schema.columns
         where table_schema = $1 and table_name = $2
         order by ordinal_position",
    )
    .bind(schema)
    .bind(object_name)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query table columns: {err}")))?;

    let columns = columns_rows
        .into_iter()
        .map(|row| ObjectColumn {
            name: row.try_get::<String, _>("column_name").unwrap_or_default(),
            data_type: row
                .try_get::<String, _>("data_type")
                .unwrap_or_else(|_| "unknown".to_string()),
            nullable: row
                .try_get::<String, _>("is_nullable")
                .map(|value| value.eq_ignore_ascii_case("YES"))
                .unwrap_or(true),
        })
        .collect::<Vec<_>>();

    let index_rows = sqlx::query(
        "select idx.relname as index_name, i.indisunique as is_unique, pg_get_indexdef(idx.oid) as index_def
         from pg_class t
         join pg_namespace n on n.oid = t.relnamespace
         join pg_index i on i.indrelid = t.oid
         join pg_class idx on idx.oid = i.indexrelid
         where n.nspname = $1 and t.relname = $2",
    )
    .bind(schema)
    .bind(object_name)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query indexes: {err}")))?;

    let indexes = index_rows
        .into_iter()
        .map(|row| {
            let index_def = row.try_get::<String, _>("index_def").unwrap_or_default();
            ObjectIndex {
                name: row.try_get::<String, _>("index_name").unwrap_or_default(),
                columns: parse_index_columns(&index_def),
                is_unique: row.try_get::<bool, _>("is_unique").unwrap_or(false),
            }
        })
        .collect::<Vec<_>>();

    let constraint_rows = sqlx::query(
        "select tc.constraint_name, tc.constraint_type, kcu.column_name
         from information_schema.table_constraints tc
         left join information_schema.key_column_usage kcu
           on tc.constraint_name = kcu.constraint_name
          and tc.table_schema = kcu.table_schema
          and tc.table_name = kcu.table_name
         where tc.table_schema = $1 and tc.table_name = $2
         order by tc.constraint_name, kcu.ordinal_position",
    )
    .bind(schema)
    .bind(object_name)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query constraints: {err}")))?;

    let constraints = collapse_constraints(constraint_rows);

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type,
        name: object_name.to_string(),
        columns,
        indexes,
        constraints,
        ddl: None,
    })
}

async fn load_index_detail(
    datasource: &DataSource,
    node_id: &str,
    database: &str,
    schema: &str,
    table: &str,
    index_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let pool = connect_pool(datasource, Some(database)).await?;
    let row = sqlx::query(
        "select i.indisunique as is_unique, pg_get_indexdef(idx.oid) as index_def
         from pg_class t
         join pg_namespace n on n.oid = t.relnamespace
         join pg_index i on i.indrelid = t.oid
         join pg_class idx on idx.oid = i.indexrelid
         where n.nspname = $1 and t.relname = $2 and idx.relname = $3",
    )
    .bind(schema)
    .bind(table)
    .bind(index_name)
    .fetch_optional(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query index detail: {err}")))?;

    let row = row.ok_or_else(|| {
        AppError::NotFound(format!(
            "postgres index not found: {database}.{schema}.{table}.{index_name}"
        ))
    })?;

    let index_def = row.try_get::<String, _>("index_def").unwrap_or_default();
    let is_unique = row.try_get::<bool, _>("is_unique").unwrap_or(false);
    let columns = parse_index_columns(&index_def);

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type: MetadataNodeType::Index,
        name: index_name.to_string(),
        columns: Vec::new(),
        indexes: vec![ObjectIndex {
            name: index_name.to_string(),
            columns,
            is_unique,
        }],
        constraints: Vec::new(),
        ddl: if index_def.trim().is_empty() {
            None
        } else {
            Some(index_def)
        },
    })
}

async fn load_trigger_detail(
    datasource: &DataSource,
    node_id: &str,
    database: &str,
    schema: &str,
    table: &str,
    trigger_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let pool = connect_pool(datasource, Some(database)).await?;
    let row = sqlx::query(
        "select pg_get_triggerdef(tg.oid, true) as trigger_def
         from pg_trigger tg
         join pg_class t on t.oid = tg.tgrelid
         join pg_namespace n on n.oid = t.relnamespace
         where n.nspname = $1 and t.relname = $2 and tg.tgname = $3 and not tg.tgisinternal",
    )
    .bind(schema)
    .bind(table)
    .bind(trigger_name)
    .fetch_optional(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query trigger detail: {err}")))?;

    let row = row.ok_or_else(|| {
        AppError::NotFound(format!(
            "postgres trigger not found: {database}.{schema}.{table}.{trigger_name}"
        ))
    })?;
    let ddl = row.try_get::<String, _>("trigger_def").ok();

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type: MetadataNodeType::Trigger,
        name: trigger_name.to_string(),
        columns: Vec::new(),
        indexes: Vec::new(),
        constraints: Vec::new(),
        ddl,
    })
}

fn collapse_constraints(rows: Vec<sqlx::postgres::PgRow>) -> Vec<ObjectConstraint> {
    let mut map: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for row in rows {
        let name = row
            .try_get::<String, _>("constraint_name")
            .unwrap_or_default();
        let kind = row
            .try_get::<String, _>("constraint_type")
            .unwrap_or_else(|_| "UNKNOWN".to_string());
        let column = row
            .try_get::<Option<String>, _>("column_name")
            .ok()
            .flatten();

        let entry = map.entry(name).or_insert((kind, Vec::new()));
        if let Some(column) = column {
            entry.1.push(column);
        }
    }

    map.into_iter()
        .map(|(name, (constraint_type, columns))| ObjectConstraint {
            name,
            constraint_type,
            columns,
        })
        .collect()
}
