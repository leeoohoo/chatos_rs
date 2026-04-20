use crate::{
    domain::{
        datasource::{
            ConnectionTestResult, DataSource, DatabaseListResponse, DatabaseSummaryResponse,
        },
        meta::{DbType, DbTypeDescriptor},
        metadata::{MetadataNodesResponse, ObjectDetailResponse, ObjectStatsResponse},
        query::{QueryExecuteRequest, QueryExecuteResponse},
    },
    error::AppResult,
};
use async_trait::async_trait;

#[async_trait]
pub trait DatabaseDriver: Send + Sync {
    fn db_type(&self) -> DbType;
    fn descriptor(&self) -> DbTypeDescriptor;

    async fn test_connection(&self, datasource: &DataSource) -> AppResult<ConnectionTestResult>;

    async fn database_summary(&self, datasource: &DataSource)
        -> AppResult<DatabaseSummaryResponse>;

    async fn list_databases(
        &self,
        datasource: &DataSource,
        keyword: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> AppResult<DatabaseListResponse>;

    async fn object_stats(
        &self,
        datasource: &DataSource,
        database: &str,
    ) -> AppResult<ObjectStatsResponse>;

    async fn list_nodes(
        &self,
        datasource: &DataSource,
        parent_id: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> AppResult<MetadataNodesResponse>;

    async fn object_detail(
        &self,
        datasource: &DataSource,
        node_id: &str,
    ) -> AppResult<ObjectDetailResponse>;

    async fn execute(
        &self,
        datasource: &DataSource,
        request: &QueryExecuteRequest,
    ) -> AppResult<QueryExecuteResponse>;
}
