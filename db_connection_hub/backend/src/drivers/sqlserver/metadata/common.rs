// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::domain::metadata::{MetadataNode, MetadataNodesResponse};
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

pub fn parse_schema_node(node_id: &str) -> Option<(String, String)> {
    metadata_common::parse_prefixed_2(node_id, "schema")
}

pub fn parse_table_node(node_id: &str) -> Option<(String, String, String)> {
    metadata_common::parse_prefixed_3(node_id, "table")
}

pub fn parse_index_node(node_id: &str) -> Option<(String, String, String, String)> {
    metadata_common::parse_prefixed_4(node_id, "index")
}

pub fn parse_trigger_node(node_id: &str) -> Option<(String, String, String, String)> {
    metadata_common::parse_prefixed_4(node_id, "trigger")
}

#[cfg(test)]
mod tests {
    use super::{parse_index_node, parse_trigger_node};

    #[test]
    fn parse_index_node_works() {
        let parsed = parse_index_node("index:orders_db:dbo:orders:IX_ORDERS_ID");
        assert!(parsed.is_some());
        let parsed = parsed.expect("index parser should return value");
        assert_eq!(parsed.0, "orders_db");
        assert_eq!(parsed.1, "dbo");
        assert_eq!(parsed.2, "orders");
        assert_eq!(parsed.3, "IX_ORDERS_ID");
    }

    #[test]
    fn parse_trigger_node_works() {
        let parsed = parse_trigger_node("trigger:orders_db:dbo:orders:TR_ORDERS_AUDIT");
        assert!(parsed.is_some());
        let parsed = parsed.expect("trigger parser should return value");
        assert_eq!(parsed.0, "orders_db");
        assert_eq!(parsed.1, "dbo");
        assert_eq!(parsed.2, "orders");
        assert_eq!(parsed.3, "TR_ORDERS_AUDIT");
    }
}
