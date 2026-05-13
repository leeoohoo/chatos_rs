use crate::domain::metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse};
use crate::drivers::metadata_common;

pub fn parse_schema_node(parent_id: &str) -> Option<(String, String)> {
    metadata_common::parse_prefixed_2(parent_id, "schema")
}

pub fn parse_table_node(node_id: &str) -> Option<(String, String, String)> {
    metadata_common::parse_prefixed_3(node_id, "table")
}

pub fn parse_relation_node(node_id: &str) -> Option<(MetadataNodeType, String, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let schema = parts.next()?;
    let name = parts.next()?;

    let node_type = match prefix {
        "table" => MetadataNodeType::Table,
        "view" => MetadataNodeType::View,
        "materialized_view" => MetadataNodeType::MaterializedView,
        "sequence" => MetadataNodeType::Sequence,
        _ => return None,
    };

    Some((
        node_type,
        database.to_string(),
        schema.to_string(),
        name.to_string(),
    ))
}

pub fn parse_index_node(node_id: &str) -> Option<(String, String, String, String)> {
    metadata_common::parse_prefixed_4(node_id, "index")
}

pub fn parse_trigger_node(node_id: &str) -> Option<(String, String, String, String)> {
    metadata_common::parse_prefixed_4(node_id, "trigger")
}

pub fn paginate_nodes(
    items: Vec<MetadataNode>,
    page: u32,
    page_size: u32,
) -> MetadataNodesResponse {
    metadata_common::paginate_nodes(items, page, page_size)
}

pub fn parse_index_columns(index_def: &str) -> Vec<String> {
    let start = index_def.find('(');
    let end = index_def.rfind(')');
    match (start, end) {
        (Some(start_idx), Some(end_idx)) if end_idx > start_idx => index_def
            [start_idx + 1..end_idx]
            .split(',')
            .map(|segment| segment.trim().trim_matches('"').to_string())
            .filter(|segment| !segment.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn make_db_node(database: &str) -> MetadataNode {
    metadata_common::make_db_node(database)
}
