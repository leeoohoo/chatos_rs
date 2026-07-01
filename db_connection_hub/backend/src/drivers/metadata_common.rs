// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::domain::metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse};

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

pub fn make_db_node(database: &str) -> MetadataNode {
    MetadataNode {
        id: make_node_id("db", &[database]),
        parent_id: "root".to_string(),
        node_type: MetadataNodeType::Database,
        display_name: database.to_string(),
        path: make_qualified_path(&[database]),
        has_children: true,
    }
}

pub fn parse_database_node(node_id: &str) -> Option<String> {
    parse_prefixed_parts::<1>(node_id, "db").map(|[database]| database)
}

pub fn make_node_id(prefix: &str, parts: &[&str]) -> String {
    let mut id = String::from(prefix);
    for part in parts {
        id.push(':');
        id.push_str(part);
    }
    id
}

pub fn make_qualified_path(parts: &[&str]) -> String {
    parts.join(".")
}

pub fn parse_prefixed_parts<const N: usize>(node_id: &str, prefix: &str) -> Option<[String; N]> {
    let parts = node_id.split(':').collect::<Vec<_>>();
    if parts.len() != N + 1 || parts.first().copied()? != prefix {
        return None;
    }

    parts
        .into_iter()
        .skip(1)
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .try_into()
        .ok()
}

pub fn parse_prefixed_2(node_id: &str, prefix: &str) -> Option<(String, String)> {
    parse_prefixed_parts::<2>(node_id, prefix).map(|[first, second]| (first, second))
}

pub fn parse_prefixed_3(node_id: &str, prefix: &str) -> Option<(String, String, String)> {
    parse_prefixed_parts::<3>(node_id, prefix).map(|[first, second, third]| (first, second, third))
}

pub fn parse_prefixed_4(node_id: &str, prefix: &str) -> Option<(String, String, String, String)> {
    parse_prefixed_parts::<4>(node_id, prefix)
        .map(|[first, second, third, fourth]| (first, second, third, fourth))
}

pub fn node_type_from_prefix(prefix: &str) -> Option<MetadataNodeType> {
    let node_type = match prefix {
        "table" => MetadataNodeType::Table,
        "view" => MetadataNodeType::View,
        "materialized_view" => MetadataNodeType::MaterializedView,
        "sequence" => MetadataNodeType::Sequence,
        "collection" => MetadataNodeType::Collection,
        "index" => MetadataNodeType::Index,
        "procedure" => MetadataNodeType::Procedure,
        "function" => MetadataNodeType::Function,
        "trigger" => MetadataNodeType::Trigger,
        "synonym" => MetadataNodeType::Synonym,
        "package" => MetadataNodeType::Package,
        _ => return None,
    };

    Some(node_type)
}

#[cfg(test)]
mod tests {
    use crate::domain::metadata::MetadataNodeType;

    use super::{
        make_node_id, node_type_from_prefix, parse_database_node, parse_prefixed_2,
        parse_prefixed_parts,
    };

    #[test]
    fn parse_prefixed_parts_rejects_extra_segments() {
        let parsed = parse_prefixed_parts::<2>("table:orders_db:orders:extra", "table");
        assert!(parsed.is_none());
    }

    #[test]
    fn parse_prefixed_helpers_reject_extra_segments() {
        let parsed = parse_prefixed_2("schema:orders_db:public:extra", "schema");
        assert!(parsed.is_none());
    }

    #[test]
    fn parse_database_node_rejects_extra_segments() {
        let parsed = parse_database_node("db:orders:extra");
        assert!(parsed.is_none());
    }

    #[test]
    fn make_node_id_joins_prefix_and_parts() {
        let id = make_node_id(
            "index",
            &["orders", "public", "orders", "idx_orders_created_at"],
        );
        assert_eq!(id, "index:orders:public:orders:idx_orders_created_at");
    }

    #[test]
    fn node_type_from_prefix_maps_known_values() {
        assert!(matches!(
            node_type_from_prefix("materialized_view"),
            Some(MetadataNodeType::MaterializedView)
        ));
        assert!(matches!(
            node_type_from_prefix("collection"),
            Some(MetadataNodeType::Collection)
        ));
        assert!(node_type_from_prefix("unknown").is_none());
    }
}
