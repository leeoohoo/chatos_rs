use std::collections::BTreeSet;

use crate::{
    domain::{
        datasource::DataSource,
        metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse},
    },
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
    let safe_page = page.max(1);
    let safe_size = page_size.clamp(1, 500);
    let total = items.len() as u64;
    let start = ((safe_page - 1) * safe_size) as usize;

    let paged = if start >= items.len() {
        Vec::new()
    } else {
        let end = (start + safe_size as usize).min(items.len());
        items[start..end].to_vec()
    };

    MetadataNodesResponse {
        items: paged,
        page: safe_page,
        page_size: safe_size,
        total,
    }
}

pub fn parse_database_node(parent_id: &str) -> Option<String> {
    parent_id
        .strip_prefix("db:")
        .map(std::string::ToString::to_string)
}

pub fn parse_schema_node(parent_id: &str) -> Option<(String, String)> {
    let mut parts = parent_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let schema = parts.next()?;
    if prefix != "schema" {
        return None;
    }
    Some((database.to_string(), schema.to_string()))
}

pub fn parse_table_node(node_id: &str) -> Option<(String, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let schema = parts.next()?;
    let table = parts.next()?;
    if prefix != "table" {
        return None;
    }
    Some((database.to_string(), schema.to_string(), table.to_string()))
}

pub fn make_db_node(database: &str) -> MetadataNode {
    MetadataNode {
        id: format!("db:{database}"),
        parent_id: "root".to_string(),
        node_type: MetadataNodeType::Database,
        display_name: database.to_string(),
        path: database.to_string(),
        has_children: true,
    }
}
