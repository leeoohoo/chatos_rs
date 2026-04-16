use crate::{
    domain::{
        datasource::DataSource,
        metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse},
    },
    error::{AppError, AppResult},
};
use sqlx::Row;

use super::{
    super::connection::connect_pool,
    common::{
        make_db_node, paginate_nodes, parse_database_node, parse_schema_node, parse_table_node,
    },
    dbs::list_databases,
};

pub async fn list_nodes(
    datasource: &DataSource,
    parent_id: Option<&str>,
    page: u32,
    page_size: u32,
) -> AppResult<MetadataNodesResponse> {
    let parent = parent_id.unwrap_or("root");

    let items = if parent == "root" {
        list_database_nodes(datasource).await?
    } else if let Some(database) = parse_database_node(parent) {
        list_schema_nodes(&database)
    } else if let Some((database, schema)) = parse_schema_node(parent) {
        list_schema_children(datasource, &database, &schema).await?
    } else if let Some((database, schema, table)) = parse_table_node(parent) {
        list_table_children(datasource, &database, &schema, &table).await?
    } else {
        Vec::new()
    };

    Ok(paginate_nodes(items, page, page_size))
}

async fn list_database_nodes(datasource: &DataSource) -> AppResult<Vec<MetadataNode>> {
    let databases = list_databases(datasource, None, 1, 10_000).await?.items;
    Ok(databases
        .into_iter()
        .map(|database| make_db_node(&database.name))
        .collect())
}

fn list_schema_nodes(database: &str) -> Vec<MetadataNode> {
    vec![MetadataNode {
        id: format!("schema:{database}:main"),
        parent_id: format!("db:{database}"),
        node_type: MetadataNodeType::Schema,
        display_name: "main".to_string(),
        path: format!("{database}.main"),
        has_children: true,
    }]
}

async fn list_schema_children(
    datasource: &DataSource,
    database: &str,
    schema: &str,
) -> AppResult<Vec<MetadataNode>> {
    if database != "main" || schema != "main" {
        return Err(AppError::NotFound(format!(
            "unsupported sqlite scope {database}.{schema}"
        )));
    }

    let pool = connect_pool(datasource).await?;

    let rows = sqlx::query(
        "select name, type from sqlite_master
         where type in ('table','view') and name not like 'sqlite_%'
         order by name",
    )
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list sqlite objects: {err}")))?;

    let items = rows
        .into_iter()
        .map(|row| {
            let name = row.try_get::<String, _>("name").unwrap_or_default();
            let object_type = row.try_get::<String, _>("type").unwrap_or_default();
            let (node_type, has_children, id) = if object_type == "view" {
                (
                    MetadataNodeType::View,
                    false,
                    format!("view:{database}:{schema}:{name}"),
                )
            } else {
                (
                    MetadataNodeType::Table,
                    true,
                    format!("table:{database}:{schema}:{name}"),
                )
            };

            MetadataNode {
                id,
                parent_id: format!("schema:{database}:{schema}"),
                node_type,
                display_name: name.clone(),
                path: format!("{database}.{schema}.{name}"),
                has_children,
            }
        })
        .collect();

    Ok(items)
}

async fn list_table_children(
    datasource: &DataSource,
    database: &str,
    schema: &str,
    table: &str,
) -> AppResult<Vec<MetadataNode>> {
    if database != "main" || schema != "main" {
        return Err(AppError::NotFound(format!(
            "unsupported sqlite table scope {database}.{schema}.{table}"
        )));
    }

    let pool = connect_pool(datasource).await?;

    let rows = sqlx::query(
        "select name, type from sqlite_master
         where tbl_name = ? and type in ('index','trigger') and name not like 'sqlite_%'
         order by type, name",
    )
    .bind(table)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list sqlite table children: {err}")))?;

    let items = rows
        .into_iter()
        .map(|row| {
            let name = row.try_get::<String, _>("name").unwrap_or_default();
            let object_type = row.try_get::<String, _>("type").unwrap_or_default();
            let node_type = if object_type == "trigger" {
                MetadataNodeType::Trigger
            } else {
                MetadataNodeType::Index
            };

            MetadataNode {
                id: format!("{object_type}:{database}:{schema}:{table}:{name}"),
                parent_id: format!("table:{database}:{schema}:{table}"),
                node_type,
                display_name: name.clone(),
                path: format!("{database}.{schema}.{table}.{name}"),
                has_children: false,
            }
        })
        .collect();

    Ok(items)
}
