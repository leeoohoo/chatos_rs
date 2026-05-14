use std::collections::HashMap;

use crate::domain::{
    datasource::DatabaseInfo,
    metadata::{MetadataNodeType, ObjectColumn, ObjectDetailResponse, ObjectStatsResponse},
};
use crate::drivers::metadata_common;

use super::{common::*, MockCatalog};

pub fn build() -> MockCatalog {
    let billing_db_id = metadata_common::make_node_id("db", &["billing"]);
    let dbo_schema_id = metadata_common::make_node_id("schema", &["billing", "dbo"]);
    let invoices_table_id =
        metadata_common::make_node_id("table", &["billing", "dbo", "invoices"]);

    let databases = vec![DatabaseInfo {
        name: "billing".to_string(),
        owner: Some("sa".to_string()),
        size_bytes: Some(1_423_111_188),
    }];

    let mut stats = HashMap::new();
    stats.insert(
        "billing".to_string(),
        ObjectStatsResponse {
            database: "billing".to_string(),
            schema_count: Some(4),
            table_count: Some(97),
            view_count: Some(33),
            materialized_view_count: None,
            collection_count: None,
            index_count: Some(352),
            procedure_count: Some(19),
            function_count: Some(14),
            trigger_count: Some(11),
            sequence_count: None,
            synonym_count: Some(9),
            package_count: None,
            partial: false,
        },
    );

    let mut children = HashMap::new();
    children.insert("root".to_string(), database_nodes(&databases));
    children.insert(
        billing_db_id.clone(),
        vec![node(
            &dbo_schema_id,
            &billing_db_id,
            MetadataNodeType::Schema,
            "dbo",
            "billing.dbo",
            true,
        )],
    );
    children.insert(
        dbo_schema_id.clone(),
        vec![node(
            &invoices_table_id,
            &dbo_schema_id,
            MetadataNodeType::Table,
            "invoices",
            "billing.dbo.invoices",
            true,
        )],
    );

    let mut details = HashMap::new();
    details.insert(
        invoices_table_id.clone(),
        ObjectDetailResponse {
            node_id: invoices_table_id,
            node_type: MetadataNodeType::Table,
            name: "invoices".to_string(),
            columns: vec![
                ObjectColumn {
                    name: "invoice_id".to_string(),
                    data_type: "bigint".to_string(),
                    nullable: false,
                },
                ObjectColumn {
                    name: "total_amount".to_string(),
                    data_type: "decimal(18,2)".to_string(),
                    nullable: false,
                },
            ],
            indexes: vec![],
            constraints: vec![],
            ddl: Some("CREATE TABLE dbo.invoices (...)".to_string()),
        },
    );

    MockCatalog {
        databases,
        stats,
        children,
        details,
    }
}
