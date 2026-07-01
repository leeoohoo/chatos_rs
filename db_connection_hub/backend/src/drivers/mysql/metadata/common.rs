// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    domain::{
        datasource::DataSource,
        metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse},
    },
    drivers::metadata_common,
    error::{AppError, AppResult},
};

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

pub fn parse_table_node(node_id: &str) -> Option<(String, String)> {
    metadata_common::parse_prefixed_2(node_id, "table")
}

pub fn parse_detail_node(node_id: &str) -> Option<(MetadataNodeType, String, String)> {
    for prefix in ["table", "view", "procedure", "function"] {
        if let Some([database, object_name]) =
            metadata_common::parse_prefixed_parts(node_id, prefix)
        {
            let node_type = metadata_common::node_type_from_prefix(prefix)?;
            return Some((node_type, database, object_name));
        }
    }

    None
}

pub fn parse_index_node(node_id: &str) -> Option<(String, String, String)> {
    metadata_common::parse_prefixed_3(node_id, "index")
}

pub fn parse_trigger_node(node_id: &str) -> Option<(String, String, String)> {
    metadata_common::parse_prefixed_3(node_id, "trigger")
}

pub fn make_db_node(database: &str) -> MetadataNode {
    metadata_common::make_db_node(database)
}

pub fn scoped_database(datasource: &DataSource) -> Option<&str> {
    datasource
        .network
        .database
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn ensure_database_in_scope(datasource: &DataSource, database: &str) -> AppResult<()> {
    if let Some(scoped) = scoped_database(datasource) {
        if !scoped.eq_ignore_ascii_case(database) {
            return Err(AppError::BadRequest(format!(
                "database {database} is out of scope for this datasource"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::domain::metadata::MetadataNodeType;

    use super::parse_detail_node;

    #[test]
    fn parse_detail_node_supports_routine_prefixes() {
        let procedure = parse_detail_node("procedure:crm:sync_customer_tags");
        assert!(matches!(
            procedure,
            Some((MetadataNodeType::Procedure, _, _))
        ));

        let function = parse_detail_node("function:crm:compute_customer_score");
        assert!(matches!(function, Some((MetadataNodeType::Function, _, _))));
    }

    #[test]
    fn parse_detail_node_rejects_legacy_routine_prefix() {
        let routine = parse_detail_node("routine:crm:sync_customer_tags");
        assert!(routine.is_none());
    }
}
