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
            name: "orders".to_string(),
            owner: Some("postgres".to_string()),
            size_bytes: Some(2_019_235_840),
        },
        DatabaseInfo {
            name: "analytics".to_string(),
            owner: Some("postgres".to_string()),
            size_bytes: Some(894_120_448),
        },
    ];

    let mut stats = HashMap::new();
    stats.insert(
        "orders".to_string(),
        ObjectStatsResponse {
            database: "orders".to_string(),
            schema_count: Some(3),
            table_count: Some(132),
            view_count: Some(28),
            materialized_view_count: Some(5),
            collection_count: None,
            index_count: Some(436),
            procedure_count: None,
            function_count: Some(23),
            trigger_count: Some(17),
            sequence_count: Some(41),
            synonym_count: None,
            package_count: None,
            partial: false,
        },
    );

    let mut children = HashMap::new();
    children.insert("root".to_string(), database_nodes(&databases));
    children.insert(
        "db:orders".to_string(),
        vec![
            node(
                "schema:orders:public",
                "db:orders",
                MetadataNodeType::Schema,
                "public",
                "orders.public",
                true,
            ),
            node(
                "schema:orders:reporting",
                "db:orders",
                MetadataNodeType::Schema,
                "reporting",
                "orders.reporting",
                true,
            ),
        ],
    );
    children.insert(
        "schema:orders:public".to_string(),
        vec![
            node(
                "table:orders:public:orders",
                "schema:orders:public",
                MetadataNodeType::Table,
                "orders",
                "orders.public.orders",
                true,
            ),
            node(
                "view:orders:public:daily_kpi",
                "schema:orders:public",
                MetadataNodeType::View,
                "daily_kpi",
                "orders.public.daily_kpi",
                false,
            ),
        ],
    );
    children.insert(
        "table:orders:public:orders".to_string(),
        vec![node(
            "index:orders:public:idx_orders_created_at",
            "table:orders:public:orders",
            MetadataNodeType::Index,
            "idx_orders_created_at",
            "orders.public.orders.idx_orders_created_at",
            false,
        )],
    );

    let mut details = HashMap::new();
    details.insert(
        "table:orders:public:orders".to_string(),
        ObjectDetailResponse {
            node_id: "table:orders:public:orders".to_string(),
            node_type: MetadataNodeType::Table,
            name: "orders".to_string(),
            columns: vec![
                ObjectColumn {
                    name: "id".to_string(),
                    data_type: "bigint".to_string(),
                    nullable: false,
                },
                ObjectColumn {
                    name: "amount".to_string(),
                    data_type: "numeric(12,2)".to_string(),
                    nullable: false,
                },
                ObjectColumn {
                    name: "created_at".to_string(),
                    data_type: "timestamp".to_string(),
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
            ddl: Some("CREATE TABLE public.orders (...)".to_string()),
        },
    );

    MockCatalog {
        databases,
        stats,
        children,
        details,
    }
}
