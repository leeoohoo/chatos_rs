use std::collections::BTreeSet;

use crate::{
    domain::{
        datasource::DataSource,
        metadata::{MetadataNode, MetadataNodesResponse},
    },
    drivers::metadata_common,
    error::AppResult,
};

use super::super::connection::probe_tcp;

pub async fn derive_databases(datasource: &DataSource) -> AppResult<Vec<String>> {
    // First-stage oracle driver performs network verification before metadata projection.
    probe_tcp(datasource).await?;
    Ok(derive_databases_without_probe(datasource))
}

pub fn derive_databases_without_probe(datasource: &DataSource) -> Vec<String> {
    let mut items = BTreeSet::new();

    if let Some(value) = datasource.network.database.as_deref() {
        if !value.trim().is_empty() {
            items.insert(value.trim().to_string());
        }
    }
    if let Some(value) = datasource.network.service_name.as_deref() {
        if !value.trim().is_empty() {
            items.insert(value.trim().to_string());
        }
    }
    if let Some(value) = datasource.network.sid.as_deref() {
        if !value.trim().is_empty() {
            items.insert(value.trim().to_string());
        }
    }

    if items.is_empty() {
        items.insert("orcl".to_string());
    }

    items.into_iter().collect()
}

pub fn derive_schemas(datasource: &DataSource) -> Vec<String> {
    let mut items = BTreeSet::new();
    if let Some(value) = datasource.auth.username.as_deref() {
        if !value.trim().is_empty() {
            items.insert(value.trim().to_uppercase());
        }
    }
    items.insert("PUBLIC".to_string());
    items.into_iter().collect()
}

pub fn paginate_nodes(
    items: Vec<MetadataNode>,
    page: u32,
    page_size: u32,
) -> MetadataNodesResponse {
    metadata_common::paginate_nodes(items, page, page_size)
}

pub fn parse_database_node(parent_id: &str) -> Option<String> {
    metadata_common::parse_database_node(parent_id)
}

pub fn parse_schema_node(parent_id: &str) -> Option<(String, String)> {
    metadata_common::parse_prefixed_2(parent_id, "schema")
}

pub fn parse_table_node(node_id: &str) -> Option<(String, String, String)> {
    metadata_common::parse_prefixed_3(node_id, "table")
}

pub fn make_db_node(database: &str) -> MetadataNode {
    metadata_common::make_db_node(database)
}
