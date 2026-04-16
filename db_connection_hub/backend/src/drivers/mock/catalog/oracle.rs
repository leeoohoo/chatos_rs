use std::collections::HashMap;

use crate::domain::{
    datasource::DatabaseInfo,
    metadata::{MetadataNodeType, ObjectColumn, ObjectDetailResponse, ObjectStatsResponse},
};

use super::{common::*, MockCatalog};

pub fn build() -> MockCatalog {
    let databases = vec![DatabaseInfo {
        name: "orclpdb1".to_string(),
        owner: Some("SYSTEM".to_string()),
        size_bytes: Some(1_123_000_002),
    }];

    let mut stats = HashMap::new();
    stats.insert(
        "orclpdb1".to_string(),
        ObjectStatsResponse {
            database: "orclpdb1".to_string(),
            schema_count: Some(6),
            table_count: Some(155),
            view_count: Some(30),
            materialized_view_count: Some(4),
            collection_count: None,
            index_count: Some(470),
            procedure_count: Some(22),
            function_count: Some(16),
            trigger_count: Some(19),
            sequence_count: Some(53),
            synonym_count: Some(41),
            package_count: Some(14),
            partial: false,
        },
    );

    let mut children = HashMap::new();
    children.insert("root".to_string(), database_nodes(&databases));
    children.insert(
        "db:orclpdb1".to_string(),
        vec![node(
            "schema:orclpdb1:APP_USER",
            "db:orclpdb1",
            MetadataNodeType::Schema,
            "APP_USER",
            "orclpdb1.APP_USER",
            true,
        )],
    );
    children.insert(
        "schema:orclpdb1:APP_USER".to_string(),
        vec![node(
            "table:orclpdb1:APP_USER:ORDERS",
            "schema:orclpdb1:APP_USER",
            MetadataNodeType::Table,
            "ORDERS",
            "orclpdb1.APP_USER.ORDERS",
            true,
        )],
    );

    let mut details = HashMap::new();
    details.insert(
        "table:orclpdb1:APP_USER:ORDERS".to_string(),
        ObjectDetailResponse {
            node_id: "table:orclpdb1:APP_USER:ORDERS".to_string(),
            node_type: MetadataNodeType::Table,
            name: "ORDERS".to_string(),
            columns: vec![
                ObjectColumn {
                    name: "ID".to_string(),
                    data_type: "NUMBER(19)".to_string(),
                    nullable: false,
                },
                ObjectColumn {
                    name: "AMOUNT".to_string(),
                    data_type: "NUMBER(12,2)".to_string(),
                    nullable: false,
                },
            ],
            indexes: vec![],
            constraints: vec![],
            ddl: Some("CREATE TABLE APP_USER.ORDERS (...)".to_string()),
        },
    );

    MockCatalog {
        databases,
        stats,
        children,
        details,
    }
}
