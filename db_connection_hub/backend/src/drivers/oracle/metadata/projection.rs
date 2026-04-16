use crate::domain::metadata::{
    MetadataNode, MetadataNodeType, ObjectColumn, ObjectConstraint, ObjectDetailResponse,
    ObjectIndex, ObjectStatsResponse,
};

#[derive(Debug, Clone)]
pub struct ParsedDetailNode {
    pub node_type: MetadataNodeType,
    pub schema: String,
    pub object_name: String,
    pub parent_table: Option<String>,
}

pub fn schema_projection_nodes(database: &str, schema: &str) -> Vec<MetadataNode> {
    vec![
        make_schema_child(
            database,
            schema,
            MetadataNodeType::Table,
            "table",
            "ORDERS",
            true,
        ),
        make_schema_child(
            database,
            schema,
            MetadataNodeType::View,
            "view",
            "VW_ACTIVE_ORDERS",
            false,
        ),
        make_schema_child(
            database,
            schema,
            MetadataNodeType::MaterializedView,
            "materialized_view",
            "MV_ORDER_DAILY",
            false,
        ),
        make_schema_child(
            database,
            schema,
            MetadataNodeType::Sequence,
            "sequence",
            "SEQ_ORDER_ID",
            false,
        ),
        make_schema_child(
            database,
            schema,
            MetadataNodeType::Procedure,
            "procedure",
            "PRC_REFRESH_ORDER_CACHE",
            false,
        ),
        make_schema_child(
            database,
            schema,
            MetadataNodeType::Function,
            "function",
            "FN_ORDER_TOTAL",
            false,
        ),
        make_schema_child(
            database,
            schema,
            MetadataNodeType::Synonym,
            "synonym",
            "SYN_ORDERS",
            false,
        ),
        make_schema_child(
            database,
            schema,
            MetadataNodeType::Package,
            "package",
            "PKG_ORDER_API",
            false,
        ),
    ]
}

pub fn table_projection_children(database: &str, schema: &str, table: &str) -> Vec<MetadataNode> {
    let normalized = table.trim().to_uppercase();
    let index_name = format!("IDX_{normalized}_ID");
    let trigger_name = format!("TRG_{normalized}_AUDIT");

    vec![
        MetadataNode {
            id: format!("index:{database}:{schema}:{normalized}:{index_name}"),
            parent_id: format!("table:{database}:{schema}:{normalized}"),
            node_type: MetadataNodeType::Index,
            display_name: index_name.clone(),
            path: format!("{database}.{schema}.{normalized}.{index_name}"),
            has_children: false,
        },
        MetadataNode {
            id: format!("trigger:{database}:{schema}:{normalized}:{trigger_name}"),
            parent_id: format!("table:{database}:{schema}:{normalized}"),
            node_type: MetadataNodeType::Trigger,
            display_name: trigger_name.clone(),
            path: format!("{database}.{schema}.{normalized}.{trigger_name}"),
            has_children: false,
        },
    ]
}

pub fn projected_object_stats(database: &str, schema_count: u64) -> ObjectStatsResponse {
    let per_schema_count = 1;

    ObjectStatsResponse {
        database: database.to_string(),
        schema_count: Some(schema_count),
        table_count: Some(schema_count * per_schema_count),
        view_count: Some(schema_count * per_schema_count),
        materialized_view_count: Some(schema_count * per_schema_count),
        collection_count: None,
        index_count: Some(schema_count * per_schema_count),
        procedure_count: Some(schema_count * per_schema_count),
        function_count: Some(schema_count * per_schema_count),
        trigger_count: Some(schema_count * per_schema_count),
        sequence_count: Some(schema_count * per_schema_count),
        synonym_count: Some(schema_count * per_schema_count),
        package_count: Some(schema_count * per_schema_count),
        partial: true,
    }
}

pub fn parse_detail_node(node_id: &str) -> Option<ParsedDetailNode> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let _database = parts.next()?;
    let schema = parts.next()?.to_string();

    match prefix {
        "table" | "view" | "materialized_view" | "sequence" | "procedure" | "function"
        | "synonym" | "package" => Some(ParsedDetailNode {
            node_type: map_node_type(prefix)?,
            schema,
            object_name: parts.next()?.to_string(),
            parent_table: None,
        }),
        "index" | "trigger" => {
            let parent_table = parts.next()?.to_string();
            Some(ParsedDetailNode {
                node_type: map_node_type(prefix)?,
                schema,
                object_name: parts.next()?.to_string(),
                parent_table: Some(parent_table),
            })
        }
        _ => None,
    }
}

pub fn build_projected_detail(node_id: &str, parsed: &ParsedDetailNode) -> ObjectDetailResponse {
    let (columns, indexes, constraints, ddl) = match parsed.node_type {
        MetadataNodeType::Table => build_table_detail(parsed),
        MetadataNodeType::View => (
            sample_view_columns(),
            Vec::new(),
            Vec::new(),
            Some(format!(
                "CREATE OR REPLACE VIEW {}.{} AS\nSELECT ORDER_ID, CUSTOMER_ID, STATUS\nFROM {}.ORDERS;",
                parsed.schema, parsed.object_name, parsed.schema
            )),
        ),
        MetadataNodeType::MaterializedView => (
            sample_view_columns(),
            Vec::new(),
            Vec::new(),
            Some(format!(
                "CREATE MATERIALIZED VIEW {}.{}\nBUILD IMMEDIATE REFRESH COMPLETE ON DEMAND\nAS SELECT ORDER_ID, STATUS FROM {}.ORDERS;",
                parsed.schema, parsed.object_name, parsed.schema
            )),
        ),
        MetadataNodeType::Sequence => (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(format!(
                "CREATE SEQUENCE {}.{} START WITH 1 INCREMENT BY 1 NOCACHE;",
                parsed.schema, parsed.object_name
            )),
        ),
        MetadataNodeType::Procedure => (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(format!(
                "CREATE OR REPLACE PROCEDURE {}.{} AS\nBEGIN\n  NULL;\nEND;",
                parsed.schema, parsed.object_name
            )),
        ),
        MetadataNodeType::Function => (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(format!(
                "CREATE OR REPLACE FUNCTION {}.{}(p_order_id NUMBER)\nRETURN NUMBER AS\nBEGIN\n  RETURN p_order_id;\nEND;",
                parsed.schema, parsed.object_name
            )),
        ),
        MetadataNodeType::Synonym => (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(format!(
                "CREATE OR REPLACE SYNONYM {}.{} FOR {}.ORDERS;",
                parsed.schema, parsed.object_name, parsed.schema
            )),
        ),
        MetadataNodeType::Package => (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(format!(
                "CREATE OR REPLACE PACKAGE {}.{} AS\n  PROCEDURE refresh_cache;\nEND;",
                parsed.schema, parsed.object_name
            )),
        ),
        MetadataNodeType::Index => {
            let table_name = parsed.parent_table.as_deref().unwrap_or("ORDERS");
            (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Some(format!(
                    "CREATE INDEX {}.{} ON {}.{}(ORDER_ID);",
                    parsed.schema, parsed.object_name, parsed.schema, table_name
                )),
            )
        }
        MetadataNodeType::Trigger => {
            let table_name = parsed.parent_table.as_deref().unwrap_or("ORDERS");
            (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Some(format!(
                    "CREATE OR REPLACE TRIGGER {}.{}\nBEFORE INSERT ON {}.{}\nFOR EACH ROW\nBEGIN\n  NULL;\nEND;",
                    parsed.schema, parsed.object_name, parsed.schema, table_name
                )),
            )
        }
        _ => (Vec::new(), Vec::new(), Vec::new(), None),
    };

    ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type: parsed.node_type.clone(),
        name: parsed.object_name.clone(),
        columns,
        indexes,
        constraints,
        ddl,
    }
}

fn make_schema_child(
    database: &str,
    schema: &str,
    node_type: MetadataNodeType,
    prefix: &str,
    name: &str,
    has_children: bool,
) -> MetadataNode {
    MetadataNode {
        id: format!("{prefix}:{database}:{schema}:{name}"),
        parent_id: format!("schema:{database}:{schema}"),
        node_type,
        display_name: name.to_string(),
        path: format!("{database}.{schema}.{name}"),
        has_children,
    }
}

fn map_node_type(prefix: &str) -> Option<MetadataNodeType> {
    let node_type = match prefix {
        "table" => MetadataNodeType::Table,
        "view" => MetadataNodeType::View,
        "materialized_view" => MetadataNodeType::MaterializedView,
        "sequence" => MetadataNodeType::Sequence,
        "procedure" => MetadataNodeType::Procedure,
        "function" => MetadataNodeType::Function,
        "synonym" => MetadataNodeType::Synonym,
        "package" => MetadataNodeType::Package,
        "index" => MetadataNodeType::Index,
        "trigger" => MetadataNodeType::Trigger,
        _ => return None,
    };

    Some(node_type)
}

fn build_table_detail(
    parsed: &ParsedDetailNode,
) -> (
    Vec<ObjectColumn>,
    Vec<ObjectIndex>,
    Vec<ObjectConstraint>,
    Option<String>,
) {
    let columns = vec![
        ObjectColumn {
            name: "ORDER_ID".to_string(),
            data_type: "NUMBER(19)".to_string(),
            nullable: false,
        },
        ObjectColumn {
            name: "CUSTOMER_ID".to_string(),
            data_type: "NUMBER(19)".to_string(),
            nullable: false,
        },
        ObjectColumn {
            name: "STATUS".to_string(),
            data_type: "VARCHAR2(32)".to_string(),
            nullable: false,
        },
        ObjectColumn {
            name: "CREATED_AT".to_string(),
            data_type: "TIMESTAMP(6)".to_string(),
            nullable: false,
        },
    ];

    let normalized_name = parsed.object_name.to_uppercase();
    let indexes = vec![ObjectIndex {
        name: format!("IDX_{normalized_name}_ID"),
        columns: vec!["ORDER_ID".to_string()],
        is_unique: false,
    }];

    let constraints = vec![
        ObjectConstraint {
            name: format!("PK_{normalized_name}"),
            constraint_type: "PRIMARY KEY".to_string(),
            columns: vec!["ORDER_ID".to_string()],
        },
        ObjectConstraint {
            name: format!("CHK_{normalized_name}_STATUS"),
            constraint_type: "CHECK".to_string(),
            columns: vec!["STATUS".to_string()],
        },
    ];

    let ddl = Some(format!(
        "CREATE TABLE {}.{} (\n  ORDER_ID NUMBER(19) NOT NULL,\n  CUSTOMER_ID NUMBER(19) NOT NULL,\n  STATUS VARCHAR2(32) NOT NULL,\n  CREATED_AT TIMESTAMP(6) NOT NULL,\n  CONSTRAINT PK_{} PRIMARY KEY (ORDER_ID)\n);",
        parsed.schema, parsed.object_name, normalized_name
    ));

    (columns, indexes, constraints, ddl)
}

fn sample_view_columns() -> Vec<ObjectColumn> {
    vec![
        ObjectColumn {
            name: "ORDER_ID".to_string(),
            data_type: "NUMBER(19)".to_string(),
            nullable: false,
        },
        ObjectColumn {
            name: "CUSTOMER_ID".to_string(),
            data_type: "NUMBER(19)".to_string(),
            nullable: false,
        },
        ObjectColumn {
            name: "STATUS".to_string(),
            data_type: "VARCHAR2(32)".to_string(),
            nullable: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use crate::domain::metadata::MetadataNodeType;

    use super::{parse_detail_node, schema_projection_nodes, table_projection_children};

    #[test]
    fn schema_projection_contains_main_object_types() {
        let nodes = schema_projection_nodes("orclpdb1", "APP_USER");
        assert!(nodes
            .iter()
            .any(|item| matches!(item.node_type, MetadataNodeType::Table)));
        assert!(nodes
            .iter()
            .any(|item| matches!(item.node_type, MetadataNodeType::View)));
        assert!(nodes
            .iter()
            .any(|item| matches!(item.node_type, MetadataNodeType::MaterializedView)));
        assert!(nodes
            .iter()
            .any(|item| matches!(item.node_type, MetadataNodeType::Procedure)));
        assert!(nodes
            .iter()
            .any(|item| matches!(item.node_type, MetadataNodeType::Function)));
        assert!(nodes
            .iter()
            .any(|item| matches!(item.node_type, MetadataNodeType::Synonym)));
        assert!(nodes
            .iter()
            .any(|item| matches!(item.node_type, MetadataNodeType::Package)));
    }

    #[test]
    fn table_projection_contains_index_and_trigger() {
        let children = table_projection_children("orclpdb1", "APP_USER", "ORDERS");

        assert!(children
            .iter()
            .any(|item| matches!(item.node_type, MetadataNodeType::Index)));
        assert!(children
            .iter()
            .any(|item| matches!(item.node_type, MetadataNodeType::Trigger)));
    }

    #[test]
    fn parse_detail_supports_trigger_node() {
        let parsed = parse_detail_node("trigger:orclpdb1:APP_USER:ORDERS:TRG_ORDERS_AUDIT");
        assert!(parsed.is_some());
        let parsed = parsed.expect("parsed trigger node should exist");
        assert!(matches!(parsed.node_type, MetadataNodeType::Trigger));
        assert_eq!(parsed.parent_table.as_deref(), Some("ORDERS"));
    }
}
