// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{domain::metadata::MetadataNodeType, drivers::metadata_common};

#[derive(Clone, Copy)]
pub(super) enum SpecialNodeKind {
    Procedure,
    Function,
    Sequence,
    Synonym,
}

pub(super) fn parse_relation_node(
    node_id: &str,
) -> Option<(MetadataNodeType, String, String, String, &'static str)> {
    if let Some([database, schema, object_name]) =
        metadata_common::parse_prefixed_parts(node_id, "table")
    {
        return Some((MetadataNodeType::Table, database, schema, object_name, "U"));
    }

    if let Some([database, schema, object_name]) =
        metadata_common::parse_prefixed_parts(node_id, "view")
    {
        return Some((MetadataNodeType::View, database, schema, object_name, "V"));
    }

    None
}

pub(super) fn parse_special_node(
    node_id: &str,
) -> Option<(MetadataNodeType, String, String, String, SpecialNodeKind)> {
    for (prefix, node_type, kind) in [
        (
            "procedure",
            MetadataNodeType::Procedure,
            SpecialNodeKind::Procedure,
        ),
        (
            "function",
            MetadataNodeType::Function,
            SpecialNodeKind::Function,
        ),
        (
            "sequence",
            MetadataNodeType::Sequence,
            SpecialNodeKind::Sequence,
        ),
        (
            "synonym",
            MetadataNodeType::Synonym,
            SpecialNodeKind::Synonym,
        ),
    ] {
        if let Some([database, schema, object_name]) =
            metadata_common::parse_prefixed_parts(node_id, prefix)
        {
            return Some((node_type, database, schema, object_name, kind));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{parse_relation_node, parse_special_node};
    use crate::domain::metadata::MetadataNodeType;

    #[test]
    fn parse_relation_node_supports_table_and_view() {
        let table = parse_relation_node("table:orders_db:dbo:orders");
        assert!(table.is_some());
        let table = table.expect("table node should parse");
        assert!(matches!(table.0, MetadataNodeType::Table));
        assert_eq!(table.4, "U");

        let view = parse_relation_node("view:orders_db:dbo:orders_view");
        assert!(view.is_some());
        let view = view.expect("view node should parse");
        assert!(matches!(view.0, MetadataNodeType::View));
        assert_eq!(view.4, "V");
    }

    #[test]
    fn parse_special_node_supports_all_sql_server_special_types() {
        let procedure = parse_special_node("procedure:orders_db:dbo:sp_refresh_orders");
        assert!(procedure.is_some());
        assert!(matches!(
            procedure.expect("procedure should parse").0,
            MetadataNodeType::Procedure
        ));

        let function = parse_special_node("function:orders_db:dbo:fn_order_total");
        assert!(function.is_some());
        assert!(matches!(
            function.expect("function should parse").0,
            MetadataNodeType::Function
        ));

        let sequence = parse_special_node("sequence:orders_db:dbo:seq_order_id");
        assert!(sequence.is_some());
        assert!(matches!(
            sequence.expect("sequence should parse").0,
            MetadataNodeType::Sequence
        ));

        let synonym = parse_special_node("synonym:orders_db:dbo:syn_order");
        assert!(synonym.is_some());
        assert!(matches!(
            synonym.expect("synonym should parse").0,
            MetadataNodeType::Synonym
        ));
    }
}
