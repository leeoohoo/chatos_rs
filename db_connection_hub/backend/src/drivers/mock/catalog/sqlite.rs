// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use crate::domain::{
    datasource::DatabaseInfo,
    metadata::{
        MetadataNodeType, ObjectColumn, ObjectConstraint, ObjectDetailResponse, ObjectIndex,
        ObjectStatsResponse,
    },
};
use crate::drivers::metadata_common;

use super::{common::*, MockCatalog};

pub fn build() -> MockCatalog {
    let main_db_id = metadata_common::make_node_id("db", &["main"]);
    let main_schema_id = metadata_common::make_node_id("schema", &["main", "main"]);
    let orders_table_id = metadata_common::make_node_id("table", &["main", "main", "orders"]);
    let created_at_index_id = metadata_common::make_node_id(
        "index",
        &["main", "main", "orders", "idx_orders_created_at"],
    );

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
        main_db_id.clone(),
        vec![node(
            &main_schema_id,
            &main_db_id,
            MetadataNodeType::Schema,
            "main",
            "main.main",
            true,
        )],
    );
    children.insert(
        main_schema_id.clone(),
        vec![node(
            &orders_table_id,
            &main_schema_id,
            MetadataNodeType::Table,
            "orders",
            "main.main.orders",
            true,
        )],
    );
    children.insert(
        orders_table_id.clone(),
        vec![node(
            &created_at_index_id,
            &orders_table_id,
            MetadataNodeType::Index,
            "idx_orders_created_at",
            "main.main.orders.idx_orders_created_at",
            false,
        )],
    );

    let mut details = HashMap::new();
    details.insert(
        orders_table_id.clone(),
        ObjectDetailResponse {
            node_id: orders_table_id,
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
