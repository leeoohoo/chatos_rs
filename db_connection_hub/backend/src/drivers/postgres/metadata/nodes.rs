// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    domain::{
        datasource::DataSource,
        metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse},
    },
    drivers::metadata_common,
    error::{AppError, AppResult},
};
use sqlx::Row;

use super::{
    super::connection::connect_pool,
    common::{
        function_display_name, make_db_node, paginate_nodes, parse_database_node,
        parse_schema_node, parse_table_node,
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
        list_schema_nodes(datasource, &database).await?
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

async fn list_schema_nodes(
    datasource: &DataSource,
    database: &str,
) -> AppResult<Vec<MetadataNode>> {
    let pool = connect_pool(datasource, Some(database)).await?;
    let rows = sqlx::query(
        "select nspname from pg_namespace
         where nspname not like 'pg_%' and nspname <> 'information_schema'
         order by nspname",
    )
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list schemas: {err}")))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let schema = row.try_get::<String, _>("nspname").unwrap_or_default();
            MetadataNode {
                id: metadata_common::make_node_id("schema", &[database, &schema]),
                parent_id: metadata_common::make_node_id("db", &[database]),
                node_type: MetadataNodeType::Schema,
                display_name: schema.clone(),
                path: metadata_common::make_qualified_path(&[database, &schema]),
                has_children: true,
            }
        })
        .collect())
}

async fn list_schema_children(
    datasource: &DataSource,
    database: &str,
    schema: &str,
) -> AppResult<Vec<MetadataNode>> {
    let pool = connect_pool(datasource, Some(database)).await?;

    let relation_rows = sqlx::query(
        "select c.relname, c.relkind
         from pg_class c
         join pg_namespace n on n.oid = c.relnamespace
         where n.nspname = $1 and c.relkind in ('r', 'v', 'm', 'S')
         order by c.relname",
    )
    .bind(schema)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list schema objects: {err}")))?;

    let function_rows = sqlx::query(
        "select p.proname, pg_get_function_identity_arguments(p.oid) as identity_args
         from pg_proc p
         join pg_namespace n on n.oid = p.pronamespace
         where n.nspname = $1
         order by p.proname",
    )
    .bind(schema)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list functions: {err}")))?;

    let mut nodes = relation_rows
        .into_iter()
        .map(|row| {
            let name = row.try_get::<String, _>("relname").unwrap_or_default();
            let relkind = row.try_get::<String, _>("relkind").unwrap_or_default();
            let (node_type, has_children) = match relkind.as_str() {
                "r" => (MetadataNodeType::Table, true),
                "v" => (MetadataNodeType::View, false),
                "m" => (MetadataNodeType::MaterializedView, false),
                "S" => (MetadataNodeType::Sequence, false),
                _ => (MetadataNodeType::Table, false),
            };
            let id = match node_type {
                MetadataNodeType::Table => {
                    metadata_common::make_node_id("table", &[database, schema, &name])
                }
                MetadataNodeType::View => {
                    metadata_common::make_node_id("view", &[database, schema, &name])
                }
                MetadataNodeType::MaterializedView => {
                    metadata_common::make_node_id("materialized_view", &[database, schema, &name])
                }
                MetadataNodeType::Sequence => {
                    metadata_common::make_node_id("sequence", &[database, schema, &name])
                }
                _ => metadata_common::make_node_id("object", &[database, schema, &name]),
            };

            MetadataNode {
                id,
                parent_id: metadata_common::make_node_id("schema", &[database, schema]),
                node_type,
                display_name: name.clone(),
                path: metadata_common::make_qualified_path(&[database, schema, &name]),
                has_children,
            }
        })
        .collect::<Vec<_>>();

    nodes.extend(function_rows.into_iter().map(|row| {
        let name = row.try_get::<String, _>("proname").unwrap_or_default();
        let identity_args = row
            .try_get::<String, _>("identity_args")
            .unwrap_or_default();
        let display_name = function_display_name(&name, &identity_args);
        MetadataNode {
            id: metadata_common::make_node_id(
                "function",
                &[database, schema, &name, &identity_args],
            ),
            parent_id: metadata_common::make_node_id("schema", &[database, schema]),
            node_type: MetadataNodeType::Function,
            display_name: display_name.clone(),
            path: metadata_common::make_qualified_path(&[database, schema, &display_name]),
            has_children: false,
        }
    }));

    Ok(nodes)
}

async fn list_table_children(
    datasource: &DataSource,
    database: &str,
    schema: &str,
    table: &str,
) -> AppResult<Vec<MetadataNode>> {
    let pool = connect_pool(datasource, Some(database)).await?;

    let index_rows = sqlx::query(
        "select idx.relname as index_name
         from pg_class t
         join pg_namespace n on n.oid = t.relnamespace
         join pg_index i on i.indrelid = t.oid
         join pg_class idx on idx.oid = i.indexrelid
         where n.nspname = $1 and t.relname = $2",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list indexes: {err}")))?;

    let trigger_rows = sqlx::query(
        "select tgname from pg_trigger tg
         join pg_class t on tg.tgrelid = t.oid
         join pg_namespace n on n.oid = t.relnamespace
         where n.nspname = $1 and t.relname = $2 and not tg.tgisinternal",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(&pool)
    .await
    .map_err(|err| AppError::BadRequest(format!("failed to list triggers: {err}")))?;

    let mut nodes = index_rows
        .into_iter()
        .map(|row| {
            let name = row.try_get::<String, _>("index_name").unwrap_or_default();
            MetadataNode {
                id: metadata_common::make_node_id("index", &[database, schema, table, &name]),
                parent_id: metadata_common::make_node_id("table", &[database, schema, table]),
                node_type: MetadataNodeType::Index,
                display_name: name.clone(),
                path: metadata_common::make_qualified_path(&[database, schema, table, &name]),
                has_children: false,
            }
        })
        .collect::<Vec<_>>();

    nodes.extend(trigger_rows.into_iter().map(|row| {
        let name = row.try_get::<String, _>("tgname").unwrap_or_default();
        MetadataNode {
            id: metadata_common::make_node_id("trigger", &[database, schema, table, &name]),
            parent_id: metadata_common::make_node_id("table", &[database, schema, table]),
            node_type: MetadataNodeType::Trigger,
            display_name: name.clone(),
            path: metadata_common::make_qualified_path(&[database, schema, table, &name]),
            has_children: false,
        }
    }));

    Ok(nodes)
}
