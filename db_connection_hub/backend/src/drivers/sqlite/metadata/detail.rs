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
    common::{parse_detail_node, parse_index_node, parse_trigger_node, quote_sqlite_ident},
};

pub async fn object_detail(
    datasource: &DataSource,
    node_id: &str,
) -> AppResult<ObjectDetailResponse> {
    if let Some((node_type, database, schema, object_name)) = parse_detail_node(node_id) {
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
    ensure_main_scope(database, schema, object_name)?;

    let pool = connect_pool(datasource).await?;
    let escaped_table = quote_sqlite_ident(object_name);

    let columns_sql = format!("pragma table_info(\"{}\")", escaped_table);
    let columns_rows = sqlx::query(&columns_sql)
        .fetch_all(&pool)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to query table columns: {err}")))?;

    let columns = columns_rows
        .iter()
        .map(|row| ObjectColumn {
            name: row.try_get::<String, _>("name").unwrap_or_default(),
            data_type: row
                .try_get::<String, _>("type")
                .unwrap_or_else(|_| "unknown".to_string()),
            nullable: row.try_get::<i64, _>("notnull").unwrap_or(0) == 0,
        })
        .collect::<Vec<_>>();

    let index_list_sql = format!("pragma index_list(\"{}\")", escaped_table);
    let index_rows = sqlx::query(&index_list_sql)
        .fetch_all(&pool)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to query indexes: {err}")))?;

    let mut indexes = Vec::new();
    for row in index_rows {
        let index_name = row.try_get::<String, _>("name").unwrap_or_default();
        if index_name.starts_with("sqlite_") {
            continue;
        }

        let unique = row.try_get::<i64, _>("unique").unwrap_or(0) == 1;
        let escaped_index = quote_sqlite_ident(&index_name);
        let index_info_sql = format!("pragma index_info(\"{}\")", escaped_index);
        let columns_rows = sqlx::query(&index_info_sql)
            .fetch_all(&pool)
            .await
            .map_err(|err| AppError::BadRequest(format!("failed to query index columns: {err}")))?;

        let index_columns = columns_rows
            .into_iter()
            .map(|index_row| index_row.try_get::<String, _>("name").unwrap_or_default())
            .collect::<Vec<_>>();

        indexes.push(ObjectIndex {
            name: index_name,
            columns: index_columns,
            is_unique: unique,
        });
    }

    let constraints = build_constraints(&columns_rows, &indexes);

    let master_type = if matches!(node_type, MetadataNodeType::View) {
        "view"
    } else {
        "table"
    };
    let ddl = sqlx::query("select sql from sqlite_master where type = ? and name = ?")
        .bind(master_type)
        .bind(object_name)
        .fetch_optional(&pool)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to load table ddl: {err}")))?
        .and_then(|row| row.try_get::<Option<String>, _>("sql").ok().flatten());

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type,
        name: object_name.to_string(),
        columns,
        indexes,
        constraints,
        ddl,
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
    ensure_main_scope(database, schema, table)?;

    let pool = connect_pool(datasource).await?;
    let escaped_index = quote_sqlite_ident(index_name);
    let index_info_sql = format!("pragma index_info(\"{}\")", escaped_index);
    let index_info_rows = sqlx::query(&index_info_sql)
        .fetch_all(&pool)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to query index detail: {err}")))?;

    if index_info_rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "sqlite index not found: {database}.{schema}.{table}.{index_name}"
        )));
    }

    let columns = index_info_rows
        .into_iter()
        .map(|row| row.try_get::<String, _>("name").unwrap_or_default())
        .collect::<Vec<_>>();

    let escaped_table = quote_sqlite_ident(table);
    let index_list_sql = format!("pragma index_list(\"{}\")", escaped_table);
    let index_rows = sqlx::query(&index_list_sql)
        .fetch_all(&pool)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to query sqlite index list: {err}")))?;
    let is_unique = index_rows.into_iter().any(|row| {
        row.try_get::<String, _>("name")
            .ok()
            .is_some_and(|name| name == index_name)
            && row.try_get::<i64, _>("unique").unwrap_or(0) == 1
    });

    let ddl = sqlx::query("select sql from sqlite_master where type = 'index' and name = ?")
        .bind(index_name)
        .fetch_optional(&pool)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to load sqlite index ddl: {err}")))?
        .and_then(|row| row.try_get::<Option<String>, _>("sql").ok().flatten());

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
    schema: &str,
    table: &str,
    trigger_name: &str,
) -> AppResult<ObjectDetailResponse> {
    ensure_main_scope(database, schema, table)?;

    let pool = connect_pool(datasource).await?;
    let ddl = sqlx::query(
        "select sql from sqlite_master where type = 'trigger' and tbl_name = ? and name = ?",
    )
    .bind(table)
    .bind(trigger_name)
    .fetch_optional(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to load sqlite trigger ddl: {err}")))?
    .and_then(|row| row.try_get::<Option<String>, _>("sql").ok().flatten());

    if ddl.is_none() {
        return Err(AppError::NotFound(format!(
            "sqlite trigger not found: {database}.{schema}.{table}.{trigger_name}"
        )));
    }

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

fn ensure_main_scope(database: &str, schema: &str, object_name: &str) -> AppResult<()> {
    if database != "main" || schema != "main" {
        return Err(AppError::NotFound(format!(
            "unsupported sqlite scope {database}.{schema}.{object_name}"
        )));
    }
    Ok(())
}

fn build_constraints(
    columns_rows: &[sqlx::sqlite::SqliteRow],
    indexes: &[ObjectIndex],
) -> Vec<ObjectConstraint> {
    let mut constraints = Vec::new();

    let mut primary_key_columns = columns_rows
        .iter()
        .filter(|row| row.try_get::<i64, _>("pk").unwrap_or(0) > 0)
        .map(|row| row.try_get::<String, _>("name").unwrap_or_default())
        .collect::<Vec<_>>();

    if !primary_key_columns.is_empty() {
        constraints.push(ObjectConstraint {
            name: "primary_key".to_string(),
            constraint_type: "PRIMARY KEY".to_string(),
            columns: std::mem::take(&mut primary_key_columns),
        });
    }

    let mut unique_constraints: HashMap<String, Vec<String>> = HashMap::new();
    for index in indexes {
        if index.is_unique {
            unique_constraints.insert(index.name.clone(), index.columns.clone());
        }
    }

    constraints.extend(
        unique_constraints
            .into_iter()
            .map(|(name, columns)| ObjectConstraint {
                name,
                constraint_type: "UNIQUE".to_string(),
                columns,
            }),
    );

    constraints
}
