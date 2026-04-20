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
        ensure_database_in_scope, make_db_node, paginate_nodes, parse_database_node,
        parse_table_node,
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
        ensure_database_in_scope(datasource, &database)?;
        list_database_children(datasource, &database).await?
    } else if let Some((database, table)) = parse_table_node(parent) {
        ensure_database_in_scope(datasource, &database)?;
        list_table_children(datasource, &database, &table).await?
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

async fn list_database_children(
    datasource: &DataSource,
    database: &str,
) -> AppResult<Vec<MetadataNode>> {
    let pool = connect_pool(datasource, Some(database)).await?;

    let table_rows = sqlx::query(
        "select table_name as name, table_type
         from information_schema.tables
         where table_schema = ?
         order by table_name",
    )
    .bind(database)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list tables/views: {err}")))?;

    let routine_rows = sqlx::query(
        "select routine_name as name, routine_type
         from information_schema.routines
         where routine_schema = ?
         order by routine_name",
    )
    .bind(database)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list routines: {err}")))?;

    let mut nodes = table_rows
        .into_iter()
        .map(|row| {
            let name = row.try_get::<String, _>("name").unwrap_or_default();
            let table_type = row.try_get::<String, _>("table_type").unwrap_or_default();
            let (node_type, has_children) = if table_type.eq_ignore_ascii_case("VIEW") {
                (MetadataNodeType::View, false)
            } else {
                (MetadataNodeType::Table, true)
            };
            let id = if matches!(node_type, MetadataNodeType::Table) {
                format!("table:{database}:{name}")
            } else {
                format!("view:{database}:{name}")
            };

            MetadataNode {
                id,
                parent_id: format!("db:{database}"),
                node_type,
                display_name: name.clone(),
                path: format!("{database}.{name}"),
                has_children,
            }
        })
        .collect::<Vec<_>>();

    nodes.extend(routine_rows.into_iter().map(|row| {
        let name = row.try_get::<String, _>("name").unwrap_or_default();
        let routine_type = row.try_get::<String, _>("routine_type").unwrap_or_default();
        let node_type = if routine_type.eq_ignore_ascii_case("FUNCTION") {
            MetadataNodeType::Function
        } else {
            MetadataNodeType::Procedure
        };

        MetadataNode {
            id: format!("routine:{database}:{name}"),
            parent_id: format!("db:{database}"),
            node_type,
            display_name: name.clone(),
            path: format!("{database}.{name}"),
            has_children: false,
        }
    }));

    Ok(nodes)
}

async fn list_table_children(
    datasource: &DataSource,
    database: &str,
    table: &str,
) -> AppResult<Vec<MetadataNode>> {
    let pool = connect_pool(datasource, Some(database)).await?;

    let index_rows = sqlx::query(
        "select distinct index_name
         from information_schema.statistics
         where table_schema = ? and table_name = ? and index_name <> 'PRIMARY'
         order by index_name",
    )
    .bind(database)
    .bind(table)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list indexes: {err}")))?;

    let trigger_rows = sqlx::query(
        "select trigger_name
         from information_schema.triggers
         where event_object_schema = ? and event_object_table = ?
         order by trigger_name",
    )
    .bind(database)
    .bind(table)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list triggers: {err}")))?;

    let mut nodes = index_rows
        .into_iter()
        .map(|row| {
            let name = row.try_get::<String, _>("index_name").unwrap_or_default();
            MetadataNode {
                id: format!("index:{database}:{table}:{name}"),
                parent_id: format!("table:{database}:{table}"),
                node_type: MetadataNodeType::Index,
                display_name: name.clone(),
                path: format!("{database}.{table}.{name}"),
                has_children: false,
            }
        })
        .collect::<Vec<_>>();

    nodes.extend(trigger_rows.into_iter().map(|row| {
        let name = row.try_get::<String, _>("trigger_name").unwrap_or_default();
        MetadataNode {
            id: format!("trigger:{database}:{table}:{name}"),
            parent_id: format!("table:{database}:{table}"),
            node_type: MetadataNodeType::Trigger,
            display_name: name.clone(),
            path: format!("{database}.{table}.{name}"),
            has_children: false,
        }
    }));

    Ok(nodes)
}
