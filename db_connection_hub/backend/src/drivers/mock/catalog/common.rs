// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::domain::{
    datasource::DatabaseInfo,
    metadata::{MetadataNode, MetadataNodeType},
};
use crate::drivers::metadata_common;

pub fn database_nodes(databases: &[DatabaseInfo]) -> Vec<MetadataNode> {
    databases
        .iter()
        .map(|db| {
            node(
                &metadata_common::make_node_id("db", &[&db.name]),
                "root",
                MetadataNodeType::Database,
                &db.name,
                &db.name,
                true,
            )
        })
        .collect()
}

pub fn node(
    id: &str,
    parent_id: &str,
    node_type: MetadataNodeType,
    display_name: &str,
    path: &str,
    has_children: bool,
) -> MetadataNode {
    MetadataNode {
        id: id.to_string(),
        parent_id: parent_id.to_string(),
        node_type,
        display_name: display_name.to_string(),
        path: path.to_string(),
        has_children,
    }
}
