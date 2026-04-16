mod common;
mod mongodb;
mod mysql;
mod oracle;
mod postgres;
mod sql_server;
mod sqlite;

use std::collections::HashMap;

use crate::domain::{
    datasource::DatabaseInfo,
    meta::DbType,
    metadata::{MetadataNode, ObjectDetailResponse, ObjectStatsResponse},
};

#[derive(Clone)]
pub struct MockCatalog {
    pub databases: Vec<DatabaseInfo>,
    pub stats: HashMap<String, ObjectStatsResponse>,
    pub children: HashMap<String, Vec<MetadataNode>>,
    pub details: HashMap<String, ObjectDetailResponse>,
}

pub fn catalog_for(db_type: DbType) -> MockCatalog {
    match db_type {
        DbType::Postgres => postgres::build(),
        DbType::MySql => mysql::build(),
        DbType::Sqlite => sqlite::build(),
        DbType::SqlServer => sql_server::build(),
        DbType::Oracle => oracle::build(),
        DbType::MongoDb => mongodb::build(),
    }
}
