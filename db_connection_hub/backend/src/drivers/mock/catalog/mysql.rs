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
    let crm_db_id = metadata_common::make_node_id("db", &["crm"]);
    let customers_table_id = metadata_common::make_node_id("table", &["crm", "customers"]);
    let customer_summary_view_id =
        metadata_common::make_node_id("view", &["crm", "customer_summary"]);
    let sync_customer_tags_procedure_id =
        metadata_common::make_node_id("procedure", &["crm", "sync_customer_tags"]);
    let customers_phone_index_id =
        metadata_common::make_node_id("index", &["crm", "customers", "idx_customers_phone"]);

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
        crm_db_id.clone(),
        vec![
            node(
                &customers_table_id,
                &crm_db_id,
                MetadataNodeType::Table,
                "customers",
                "crm.customers",
                true,
            ),
            node(
                &customer_summary_view_id,
                &crm_db_id,
                MetadataNodeType::View,
                "customer_summary",
                "crm.customer_summary",
                false,
            ),
            node(
                &sync_customer_tags_procedure_id,
                &crm_db_id,
                MetadataNodeType::Procedure,
                "sync_customer_tags",
                "crm.sync_customer_tags",
                false,
            ),
        ],
    );
    children.insert(
        customers_table_id.clone(),
        vec![node(
            &customers_phone_index_id,
            &customers_table_id,
            MetadataNodeType::Index,
            "idx_customers_phone",
            "crm.customers.idx_customers_phone",
            false,
        )],
    );

    let mut details = HashMap::new();
    details.insert(
        customers_table_id.clone(),
        ObjectDetailResponse {
            node_id: customers_table_id,
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
