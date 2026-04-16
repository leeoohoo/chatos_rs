use std::collections::HashMap;

use crate::domain::{
    datasource::DatabaseInfo,
    metadata::{
        MetadataNodeType, ObjectColumn, ObjectConstraint, ObjectDetailResponse, ObjectIndex,
        ObjectStatsResponse,
    },
};

use super::{common::*, MockCatalog};

pub fn build() -> MockCatalog {
    let databases = vec![
        DatabaseInfo {
            name: "crm".to_string(),
            owner: Some("root".to_string()),
            size_bytes: Some(523_121_311),
        },
        DatabaseInfo {
            name: "billing".to_string(),
            owner: Some("root".to_string()),
            size_bytes: Some(221_120_990),
        },
    ];

    let mut stats = HashMap::new();
    stats.insert(
        "crm".to_string(),
        ObjectStatsResponse {
            database: "crm".to_string(),
            schema_count: None,
            table_count: Some(88),
            view_count: Some(12),
            materialized_view_count: None,
            collection_count: None,
            index_count: Some(231),
            procedure_count: Some(7),
            function_count: Some(5),
            trigger_count: Some(8),
            sequence_count: None,
            synonym_count: None,
            package_count: None,
            partial: false,
        },
    );

    let mut children = HashMap::new();
    children.insert("root".to_string(), database_nodes(&databases));
    children.insert(
        "db:crm".to_string(),
        vec![
            node(
                "table:crm:customers",
                "db:crm",
                MetadataNodeType::Table,
                "customers",
                "crm.customers",
                true,
            ),
            node(
                "view:crm:customer_summary",
                "db:crm",
                MetadataNodeType::View,
                "customer_summary",
                "crm.customer_summary",
                false,
            ),
            node(
                "procedure:crm:sync_customer_tags",
                "db:crm",
                MetadataNodeType::Procedure,
                "sync_customer_tags",
                "crm.sync_customer_tags",
                false,
            ),
        ],
    );
    children.insert(
        "table:crm:customers".to_string(),
        vec![node(
            "index:crm:idx_customers_phone",
            "table:crm:customers",
            MetadataNodeType::Index,
            "idx_customers_phone",
            "crm.customers.idx_customers_phone",
            false,
        )],
    );

    let mut details = HashMap::new();
    details.insert(
        "table:crm:customers".to_string(),
        ObjectDetailResponse {
            node_id: "table:crm:customers".to_string(),
            node_type: MetadataNodeType::Table,
            name: "customers".to_string(),
            columns: vec![
                ObjectColumn {
                    name: "id".to_string(),
                    data_type: "bigint".to_string(),
                    nullable: false,
                },
                ObjectColumn {
                    name: "phone".to_string(),
                    data_type: "varchar(32)".to_string(),
                    nullable: true,
                },
            ],
            indexes: vec![ObjectIndex {
                name: "idx_customers_phone".to_string(),
                columns: vec!["phone".to_string()],
                is_unique: false,
            }],
            constraints: vec![ObjectConstraint {
                name: "PRIMARY".to_string(),
                constraint_type: "PRIMARY KEY".to_string(),
                columns: vec!["id".to_string()],
            }],
            ddl: Some("CREATE TABLE crm.customers (...)".to_string()),
        },
    );

    MockCatalog {
        databases,
        stats,
        children,
        details,
    }
}
