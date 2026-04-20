use crate::{
    domain::{
        datasource::DataSource,
        metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse},
    },
    error::AppResult,
};

use super::common::{
    derive_databases, derive_schemas, make_db_node, paginate_nodes, parse_database_node,
    parse_schema_node, parse_table_node,
};
use super::projection::{schema_projection_nodes, table_projection_children};

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
        list_schema_children(&database, &schema)
    } else if let Some((database, schema, table)) = parse_table_node(parent) {
        list_table_children(&database, &schema, &table)
    } else {
        Vec::new()
    };

    Ok(paginate_nodes(items, page, page_size))
}

async fn list_database_nodes(datasource: &DataSource) -> AppResult<Vec<MetadataNode>> {
    let databases = derive_databases(datasource).await?;
    Ok(databases
        .into_iter()
        .map(|database| make_db_node(&database))
        .collect())
}

async fn list_schema_nodes(
    datasource: &DataSource,
    database: &str,
) -> AppResult<Vec<MetadataNode>> {
    let schemas = derive_schemas(datasource);
    Ok(schemas
        .into_iter()
        .map(|schema| MetadataNode {
            id: format!("schema:{database}:{schema}"),
            parent_id: format!("db:{database}"),
            node_type: MetadataNodeType::Schema,
            display_name: schema.clone(),
            path: format!("{database}.{schema}"),
            has_children: true,
        })
        .collect())
}

fn list_schema_children(database: &str, schema: &str) -> Vec<MetadataNode> {
    schema_projection_nodes(database, schema)
}

fn list_table_children(database: &str, schema: &str, table: &str) -> Vec<MetadataNode> {
    table_projection_children(database, schema, table)
}
