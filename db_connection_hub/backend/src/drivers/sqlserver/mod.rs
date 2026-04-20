mod connection;
mod descriptor;
mod metadata;
mod query_exec;

use async_trait::async_trait;

use crate::{
    domain::{
        datasource::{
            ConnectionTestResult, DataSource, DatabaseListResponse, DatabaseSummaryResponse,
        },
        meta::{DbType, DbTypeDescriptor},
        metadata::{MetadataNodesResponse, ObjectDetailResponse, ObjectStatsResponse},
        query::{QueryExecuteRequest, QueryExecuteResponse},
    },
    drivers::traits::DatabaseDriver,
    error::AppResult,
};

pub struct SqlServerDriver;

impl SqlServerDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DatabaseDriver for SqlServerDriver {
    fn db_type(&self) -> DbType {
        DbType::SqlServer
    }

    fn descriptor(&self) -> DbTypeDescriptor {
        descriptor::sqlserver_descriptor()
    }

    async fn test_connection(&self, datasource: &DataSource) -> AppResult<ConnectionTestResult> {
        connection::test_connection(datasource).await
    }

    async fn database_summary(
        &self,
        datasource: &DataSource,
    ) -> AppResult<DatabaseSummaryResponse> {
        metadata::database_summary(datasource).await
    }

    async fn list_databases(
        &self,
        datasource: &DataSource,
        keyword: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> AppResult<DatabaseListResponse> {
        metadata::list_databases(datasource, keyword, page, page_size).await
    }

    async fn object_stats(
        &self,
        datasource: &DataSource,
        database: &str,
    ) -> AppResult<ObjectStatsResponse> {
        metadata::object_stats(datasource, database).await
    }

    async fn list_nodes(
        &self,
        datasource: &DataSource,
        parent_id: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> AppResult<MetadataNodesResponse> {
        metadata::list_nodes(datasource, parent_id, page, page_size).await
    }

    async fn object_detail(
        &self,
        datasource: &DataSource,
        node_id: &str,
    ) -> AppResult<ObjectDetailResponse> {
        metadata::object_detail(datasource, node_id).await
    }

    async fn execute(
        &self,
        datasource: &DataSource,
        request: &QueryExecuteRequest,
    ) -> AppResult<QueryExecuteResponse> {
        query_exec::execute(datasource, request).await
    }
}
