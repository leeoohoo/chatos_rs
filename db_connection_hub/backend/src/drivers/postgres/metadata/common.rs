use crate::domain::metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse};
use crate::drivers::metadata_common;

pub fn parse_database_node(node_id: &str) -> Option<String> {
    metadata_common::parse_database_node(node_id)
}

pub fn parse_schema_node(parent_id: &str) -> Option<(String, String)> {
    metadata_common::parse_prefixed_2(parent_id, "schema")
}

pub fn parse_table_node(node_id: &str) -> Option<(String, String, String)> {
    metadata_common::parse_prefixed_3(node_id, "table")
}

pub fn parse_relation_node(node_id: &str) -> Option<(MetadataNodeType, String, String, String)> {
    for prefix in ["table", "view", "materialized_view", "sequence"] {
        if let Some([database, schema, name]) = metadata_common::parse_prefixed_parts(node_id, prefix)
        {
            let node_type = metadata_common::node_type_from_prefix(prefix)?;
            return Some((node_type, database, schema, name));
        }
    }

    None
}

pub fn parse_index_node(node_id: &str) -> Option<(String, String, String, String)> {
    metadata_common::parse_prefixed_4(node_id, "index")
}

pub fn parse_trigger_node(node_id: &str) -> Option<(String, String, String, String)> {
    metadata_common::parse_prefixed_4(node_id, "trigger")
}

pub fn parse_function_node(node_id: &str) -> Option<(String, String, String, String)> {
    metadata_common::parse_prefixed_4(node_id, "function")
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

pub fn function_display_name(name: &str, identity_args: &str) -> String {
    let normalized_args = identity_args.trim();
    if normalized_args.is_empty() {
        format!("{name}()")
    } else {
        format!("{name}({normalized_args})")
    }
}

#[cfg(test)]
mod tests {
    use super::{function_display_name, parse_function_node};

    #[test]
    fn parse_function_node_supports_identity_arguments() {
        let parsed = parse_function_node(
            "function:orders:public:recalculate_tax:customer_id bigint, region text",
        );
        assert!(parsed.is_some());
        let parsed = parsed.expect("function node should parse");
        assert_eq!(parsed.0, "orders");
        assert_eq!(parsed.1, "public");
        assert_eq!(parsed.2, "recalculate_tax");
        assert_eq!(parsed.3, "customer_id bigint, region text");
    }

    #[test]
    fn function_display_name_handles_empty_signature() {
        assert_eq!(function_display_name("refresh_cache", ""), "refresh_cache()");
        assert_eq!(
            function_display_name("refresh_cache", "customer_id bigint"),
            "refresh_cache(customer_id bigint)"
        );
    }
}
