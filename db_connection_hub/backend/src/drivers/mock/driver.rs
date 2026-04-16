use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    domain::{
        datasource::{
            ConnectionTestResult, ConnectionTestStageResult, DataSource, DatabaseListResponse,
            DatabaseSummaryResponse,
        },
        meta::{DbType, DbTypeDescriptor},
        metadata::{MetadataNodesResponse, ObjectDetailResponse, ObjectStatsResponse},
        query::{QueryColumn, QueryExecuteRequest, QueryExecuteResponse},
    },
    drivers::traits::DatabaseDriver,
    error::{AppError, AppResult},
};

use super::{
    catalog::catalog_for,
    descriptors,
    validation::{
        mock_server_version, validate_auth_payload, validate_network_payload, validate_tls_payload,
    },
};

pub fn build_mock_drivers() -> Vec<Arc<dyn DatabaseDriver>> {
    descriptors::all()
        .into_iter()
        .map(|descriptor| Arc::new(MockDriver::new(descriptor)) as Arc<dyn DatabaseDriver>)
        .collect()
}

pub struct MockDriver {
    descriptor: DbTypeDescriptor,
}

impl MockDriver {
    pub fn new(descriptor: DbTypeDescriptor) -> Self {
        Self { descriptor }
    }
}

#[async_trait]
impl DatabaseDriver for MockDriver {
    fn db_type(&self) -> DbType {
        self.descriptor.db_type
    }

    fn descriptor(&self) -> DbTypeDescriptor {
        self.descriptor.clone()
    }

    async fn test_connection(&self, datasource: &DataSource) -> AppResult<ConnectionTestResult> {
        validate_auth_payload(&datasource.auth)?;
        validate_network_payload(datasource)?;
        validate_tls_payload(datasource)?;

        Ok(ConnectionTestResult {
            ok: true,
            latency_ms: 38,
            server_version: Some(mock_server_version(self.db_type()).to_string()),
            auth_mode: datasource.auth.mode,
            checks: vec![
                ConnectionTestStageResult {
                    stage: "network".to_string(),
                    ok: true,
                    message: None,
                },
                ConnectionTestStageResult {
                    stage: "tls".to_string(),
                    ok: true,
                    message: None,
                },
                ConnectionTestStageResult {
                    stage: "auth".to_string(),
                    ok: true,
                    message: None,
                },
                ConnectionTestStageResult {
                    stage: "metadata_permission".to_string(),
                    ok: true,
                    message: None,
                },
            ],
            error_code: None,
            message: None,
            stage: None,
        })
    }

    async fn database_summary(
        &self,
        _datasource: &DataSource,
    ) -> AppResult<DatabaseSummaryResponse> {
        let catalog = catalog_for(self.db_type());
        let total = catalog.databases.len() as u64;

        Ok(DatabaseSummaryResponse {
            database_count: total,
            visible_database_count: total,
            visibility_scope: "full".to_string(),
        })
    }

    async fn list_databases(
        &self,
        _datasource: &DataSource,
        keyword: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> AppResult<DatabaseListResponse> {
        let catalog = catalog_for(self.db_type());
        let mut items = catalog.databases;

        if let Some(keyword) = keyword {
            let lowered = keyword.to_lowercase();
            items.retain(|item| item.name.to_lowercase().contains(&lowered));
        }

        let (paged_items, total) = paginate(&items, page, page_size);

        Ok(DatabaseListResponse {
            items: paged_items,
            page,
            page_size,
            total,
        })
    }

    async fn object_stats(
        &self,
        _datasource: &DataSource,
        database: &str,
    ) -> AppResult<ObjectStatsResponse> {
        let catalog = catalog_for(self.db_type());
        catalog
            .stats
            .get(database)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("database {database} not found")))
    }

    async fn list_nodes(
        &self,
        _datasource: &DataSource,
        parent_id: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> AppResult<MetadataNodesResponse> {
        let catalog = catalog_for(self.db_type());
        let key = parent_id.unwrap_or("root");
        let nodes = catalog.children.get(key).cloned().unwrap_or_default();
        let (items, total) = paginate(&nodes, page, page_size);

        Ok(MetadataNodesResponse {
            items,
            page,
            page_size,
            total,
        })
    }

    async fn object_detail(
        &self,
        _datasource: &DataSource,
        node_id: &str,
    ) -> AppResult<ObjectDetailResponse> {
        let catalog = catalog_for(self.db_type());
        catalog
            .details
            .get(node_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("metadata node {node_id} not found")))
    }

    async fn execute(
        &self,
        _datasource: &DataSource,
        request: &QueryExecuteRequest,
    ) -> AppResult<QueryExecuteResponse> {
        if request.sql.trim().is_empty() {
            return Err(AppError::BadRequest("sql cannot be empty".to_string()));
        }

        let _timeout_ms = request.timeout_ms.unwrap_or(10_000);
        let max_rows = request.max_rows.unwrap_or(1_000).clamp(1, 10_000) as usize;

        let mut rows = vec![
            vec![
                json!(1),
                json!(88.5),
                json!(request
                    .database
                    .clone()
                    .unwrap_or_else(|| "default".to_string())),
            ],
            vec![
                json!(2),
                json!(19.0),
                json!(request
                    .database
                    .clone()
                    .unwrap_or_else(|| "default".to_string())),
            ],
        ];
        rows.truncate(max_rows.min(rows.len()));

        Ok(QueryExecuteResponse {
            query_id: format!("q_{}", Uuid::new_v4().simple()),
            columns: vec![
                QueryColumn {
                    name: "id".to_string(),
                    type_name: "bigint".to_string(),
                },
                QueryColumn {
                    name: "amount".to_string(),
                    type_name: "decimal".to_string(),
                },
                QueryColumn {
                    name: "database".to_string(),
                    type_name: "text".to_string(),
                },
            ],
            row_count: rows.len() as u64,
            rows,
            elapsed_ms: 24,
        })
    }
}

fn paginate<T: Clone>(items: &[T], page: u32, page_size: u32) -> (Vec<T>, u64) {
    let safe_page = page.max(1);
    let safe_page_size = page_size.clamp(1, 500);
    let total = items.len() as u64;

    let start = ((safe_page - 1) * safe_page_size) as usize;
    if start >= items.len() {
        return (Vec::new(), total);
    }

    let end = (start + safe_page_size as usize).min(items.len());
    (items[start..end].to_vec(), total)
}
