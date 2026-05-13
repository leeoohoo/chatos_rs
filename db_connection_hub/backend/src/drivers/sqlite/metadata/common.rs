use crate::domain::metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse};
use crate::drivers::metadata_common;

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

pub fn parse_detail_node(node_id: &str) -> Option<(MetadataNodeType, String, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let schema = parts.next()?;
    let object_name = parts.next()?;

    let node_type = match prefix {
        "table" => MetadataNodeType::Table,
        "view" => MetadataNodeType::View,
        _ => return None,
    };

    Some((
        node_type,
        database.to_string(),
        schema.to_string(),
        object_name.to_string(),
    ))
}

pub fn parse_index_node(node_id: &str) -> Option<(String, String, String, String)> {
    metadata_common::parse_prefixed_4(node_id, "index")
}

pub fn parse_trigger_node(node_id: &str) -> Option<(String, String, String, String)> {
    metadata_common::parse_prefixed_4(node_id, "trigger")
}

pub fn quote_sqlite_ident(value: &str) -> String {
    value.replace('"', "\"\"")
}

pub fn make_db_node(database: &str) -> MetadataNode {
    metadata_common::make_db_node(database)
}
