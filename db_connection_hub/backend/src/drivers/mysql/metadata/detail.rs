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
    common::{ensure_database_in_scope, parse_detail_node, parse_index_node, parse_trigger_node},
};

pub async fn object_detail(
    datasource: &DataSource,
    node_id: &str,
) -> AppResult<ObjectDetailResponse> {
    if let Some((node_type, database, object_name)) = parse_detail_node(node_id) {
        ensure_database_in_scope(datasource, &database)?;
        return load_relation_detail(datasource, node_id, node_type, &database, &object_name).await;
    }

    if let Some((database, table, index_name)) = parse_index_node(node_id) {
        ensure_database_in_scope(datasource, &database)?;
        return load_index_detail(datasource, node_id, &database, &table, &index_name).await;
    }

    if let Some((database, table, trigger_name)) = parse_trigger_node(node_id) {
        ensure_database_in_scope(datasource, &database)?;
        return load_trigger_detail(datasource, node_id, &database, &table, &trigger_name).await;
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
    object_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let pool = connect_pool(datasource, Some(database)).await?;

    let columns_rows = sqlx::query(
        "select column_name, column_type, is_nullable
         from information_schema.columns
         where table_schema = ? and table_name = ?
         order by ordinal_position",
    )
    .bind(database)
    .bind(object_name)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query table columns: {err}")))?;

    let columns = columns_rows
        .into_iter()
        .map(|row| ObjectColumn {
            name: row.try_get::<String, _>("column_name").unwrap_or_default(),
            data_type: row
                .try_get::<String, _>("column_type")
                .unwrap_or_else(|_| "unknown".to_string()),
            nullable: row
                .try_get::<String, _>("is_nullable")
                .map(|value| value.eq_ignore_ascii_case("YES"))
                .unwrap_or(true),
        })
        .collect::<Vec<_>>();

    let index_rows = sqlx::query(
        "select index_name, non_unique, seq_in_index, column_name
         from information_schema.statistics
         where table_schema = ? and table_name = ?
         order by index_name, seq_in_index",
    )
    .bind(database)
    .bind(object_name)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query indexes: {err}")))?;

    let indexes = collapse_indexes(index_rows);

    let constraints_rows = sqlx::query(
        "select tc.constraint_name, tc.constraint_type, kcu.column_name
         from information_schema.table_constraints tc
         left join information_schema.key_column_usage kcu
           on tc.constraint_schema = kcu.constraint_schema
          and tc.table_name = kcu.table_name
          and tc.constraint_name = kcu.constraint_name
         where tc.table_schema = ? and tc.table_name = ?
         order by tc.constraint_name, kcu.ordinal_position",
    )
    .bind(database)
    .bind(object_name)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query constraints: {err}")))?;

    let constraints = collapse_constraints(constraints_rows);

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
    table: &str,
    index_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let pool = connect_pool(datasource, Some(database)).await?;
    let rows = sqlx::query(
        "select non_unique, seq_in_index, column_name
         from information_schema.statistics
         where table_schema = ? and table_name = ? and index_name = ?
         order by seq_in_index",
    )
    .bind(database)
    .bind(table)
    .bind(index_name)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query index detail: {err}")))?;

    if rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "mysql index not found: {database}.{table}.{index_name}"
        )));
    }

    let is_unique = rows[0].try_get::<i64, _>("non_unique").unwrap_or(1) == 0;
    let columns = rows
        .into_iter()
        .filter_map(|row| {
            row.try_get::<Option<String>, _>("column_name")
                .ok()
                .flatten()
        })
        .collect::<Vec<_>>();
    let ddl = Some(format!(
        "CREATE {}INDEX `{}` ON `{}`.`{}` ({});",
        if is_unique { "UNIQUE " } else { "" },
        index_name,
        database,
        table,
        columns
            .iter()
            .map(|column| format!("`{column}`"))
            .collect::<Vec<_>>()
            .join(", ")
    ));

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
        ddl,
    })
}

async fn load_trigger_detail(
    datasource: &DataSource,
    node_id: &str,
    database: &str,
    table: &str,
    trigger_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let pool = connect_pool(datasource, Some(database)).await?;
    let row = sqlx::query(
        "select action_timing, event_manipulation, action_statement
         from information_schema.triggers
         where trigger_schema = ? and event_object_table = ? and trigger_name = ?",
    )
    .bind(database)
    .bind(table)
    .bind(trigger_name)
    .fetch_optional(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to query trigger detail: {err}")))?;

    let row = row.ok_or_else(|| {
        AppError::NotFound(format!(
            "mysql trigger not found: {database}.{table}.{trigger_name}"
        ))
    })?;

    let timing = row
        .try_get::<String, _>("action_timing")
        .unwrap_or_else(|_| "BEFORE".to_string());
    let event = row
        .try_get::<String, _>("event_manipulation")
        .unwrap_or_else(|_| "INSERT".to_string());
    let action = row
        .try_get::<String, _>("action_statement")
        .unwrap_or_else(|_| "BEGIN END".to_string());

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type: MetadataNodeType::Trigger,
        name: trigger_name.to_string(),
        columns: Vec::new(),
        indexes: Vec::new(),
        constraints: Vec::new(),
        ddl: Some(format!(
            "CREATE TRIGGER `{}` {} {} ON `{}`.`{}` FOR EACH ROW {}",
            trigger_name, timing, event, database, table, action
        )),
    })
}

fn collapse_indexes(rows: Vec<sqlx::mysql::MySqlRow>) -> Vec<ObjectIndex> {
    let mut map: HashMap<String, (bool, Vec<String>)> = HashMap::new();

    for row in rows {
        let index_name = row.try_get::<String, _>("index_name").unwrap_or_default();
        let non_unique = row.try_get::<i64, _>("non_unique").unwrap_or(1);
        let column = row.try_get::<String, _>("column_name").unwrap_or_default();

        let entry = map
            .entry(index_name)
            .or_insert((non_unique == 0, Vec::new()));
        entry.1.push(column);
    }

    map.into_iter()
        .map(|(name, (is_unique, columns))| ObjectIndex {
            name,
            columns,
            is_unique,
        })
        .collect()
}

fn collapse_constraints(rows: Vec<sqlx::mysql::MySqlRow>) -> Vec<ObjectConstraint> {
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
