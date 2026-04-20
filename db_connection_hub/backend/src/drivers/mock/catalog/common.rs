use crate::domain::{
    datasource::DatabaseInfo,
    metadata::{MetadataNode, MetadataNodeType},
};

pub fn database_nodes(databases: &[DatabaseInfo]) -> Vec<MetadataNode> {
    databases
        .iter()
        .map(|db| {
            node(
                &format!("db:{}", db.name),
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
