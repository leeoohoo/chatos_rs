use crate::domain::metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse};
use crate::drivers::metadata_common;

pub fn paginate_nodes(
    items: Vec<MetadataNode>,
    page: u32,
    page_size: u32,
) -> MetadataNodesResponse {
    metadata_common::paginate_nodes(items, page, page_size)
}

pub fn make_db_node(database: &str) -> MetadataNode {
    metadata_common::make_db_node(database)
}

pub fn parse_database_node(node_id: &str) -> Option<String> {
    metadata_common::parse_database_node(node_id)
}

pub fn parse_collection_node(node_id: &str) -> Option<(String, String)> {
    metadata_common::parse_prefixed_2(node_id, "collection")
}

pub fn parse_detail_node(node_id: &str) -> Option<(MetadataNodeType, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let name = parts.next()?;

    let node_type = match prefix {
        "collection" => MetadataNodeType::Collection,
        "view" => MetadataNodeType::View,
        _ => return None,
    };

    Some((node_type, database.to_string(), name.to_string()))
}

pub fn parse_index_node(node_id: &str) -> Option<(String, String, String)> {
    metadata_common::parse_prefixed_3(node_id, "index")
}
