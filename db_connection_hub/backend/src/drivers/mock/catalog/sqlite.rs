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
    let databases = vec![DatabaseInfo {
        name: "main".to_string(),
        owner: None,
        size_bytes: Some(32_120_512),
    }];

    let mut stats = HashMap::new();
    stats.insert(
        "main".to_string(),
        ObjectStatsResponse {
            database: "main".to_string(),
            schema_count: Some(1),
            table_count: Some(22),
            view_count: Some(4),
            materialized_view_count: None,
            collection_count: None,
            index_count: Some(31),
            procedure_count: None,
            function_count: None,
            trigger_count: Some(5),
            sequence_count: None,
            synonym_count: None,
            package_count: None,
            partial: false,
        },
    );

    let mut children = HashMap::new();
    children.insert("root".to_string(), database_nodes(&databases));
    children.insert(
        "db:main".to_string(),
        vec![node(
            "schema:main:main",
            "db:main",
            MetadataNodeType::Schema,
            "main",
            "main.main",
            true,
        )],
    );
    children.insert(
        "schema:main:main".to_string(),
        vec![node(
            "table:main:orders",
            "schema:main:main",
            MetadataNodeType::Table,
            "orders",
            "main.orders",
            true,
        )],
    );
    children.insert(
        "table:main:orders".to_string(),
        vec![node(
            "index:main:idx_orders_created_at",
            "table:main:orders",
            MetadataNodeType::Index,
            "idx_orders_created_at",
            "main.orders.idx_orders_created_at",
            false,
        )],
    );

    let mut details = HashMap::new();
    details.insert(
        "table:main:orders".to_string(),
        ObjectDetailResponse {
            node_id: "table:main:orders".to_string(),
            node_type: MetadataNodeType::Table,
            name: "orders".to_string(),
            columns: vec![
                ObjectColumn {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                },
                ObjectColumn {
                    name: "amount".to_string(),
                    data_type: "REAL".to_string(),
                    nullable: false,
                },
            ],
            indexes: vec![ObjectIndex {
                name: "idx_orders_created_at".to_string(),
                columns: vec!["created_at".to_string()],
                is_unique: false,
            }],
            constraints: vec![ObjectConstraint {
                name: "pk_orders".to_string(),
                constraint_type: "PRIMARY KEY".to_string(),
                columns: vec!["id".to_string()],
            }],
            ddl: Some("CREATE TABLE orders (...)".to_string()),
        },
    );

    MockCatalog {
        databases,
        stats,
        children,
        details,
    }
}
