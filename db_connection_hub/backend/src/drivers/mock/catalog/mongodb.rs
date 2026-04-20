use std::collections::HashMap;

use crate::domain::{
    datasource::DatabaseInfo,
    metadata::{
        MetadataNodeType, ObjectColumn, ObjectDetailResponse, ObjectIndex, ObjectStatsResponse,
    },
};

use super::{common::*, MockCatalog};

pub fn build() -> MockCatalog {
    let databases = vec![
        DatabaseInfo {
            name: "orders".to_string(),
            owner: None,
            size_bytes: Some(812_000_120),
        },
        DatabaseInfo {
            name: "events".to_string(),
            owner: None,
            size_bytes: Some(322_110_005),
        },
    ];

    let mut stats = HashMap::new();
    stats.insert(
        "orders".to_string(),
        ObjectStatsResponse {
            database: "orders".to_string(),
            schema_count: None,
            table_count: None,
            view_count: Some(4),
            materialized_view_count: None,
            collection_count: Some(68),
            index_count: Some(190),
            procedure_count: None,
            function_count: None,
            trigger_count: None,
            sequence_count: None,
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
                "collection:orders:orders",
                "db:orders",
                MetadataNodeType::Collection,
                "orders",
                "orders.orders",
                true,
            ),
            node(
                "view:orders:orders_daily_summary",
                "db:orders",
                MetadataNodeType::View,
                "orders_daily_summary",
                "orders.orders_daily_summary",
                false,
            ),
        ],
    );
    children.insert(
        "collection:orders:orders".to_string(),
        vec![node(
            "index:orders:orders:created_at_1",
            "collection:orders:orders",
            MetadataNodeType::Index,
            "created_at_1",
            "orders.orders.created_at_1",
            false,
        )],
    );

    let mut details = HashMap::new();
    details.insert(
        "collection:orders:orders".to_string(),
        ObjectDetailResponse {
            node_id: "collection:orders:orders".to_string(),
            node_type: MetadataNodeType::Collection,
            name: "orders".to_string(),
            columns: vec![
                ObjectColumn {
                    name: "_id".to_string(),
                    data_type: "ObjectId".to_string(),
                    nullable: false,
                },
                ObjectColumn {
                    name: "amount".to_string(),
                    data_type: "number".to_string(),
                    nullable: true,
                },
            ],
            indexes: vec![ObjectIndex {
                name: "created_at_1".to_string(),
                columns: vec!["created_at".to_string()],
                is_unique: false,
            }],
            constraints: vec![],
            ddl: None,
        },
    );

    MockCatalog {
        databases,
        stats,
        children,
        details,
    }
}
