use crate::{
    domain::query::{QueryCancelResponse, QueryExecuteRequest, QueryExecuteResponse},
    drivers::registry::DriverRegistry,
    error::{AppError, AppResult},
    repository::datasource_repo::DataSourceRepository,
};
use std::sync::Arc;

pub struct QueryService {
    repo: Arc<dyn DataSourceRepository>,
    registry: Arc<DriverRegistry>,
}

impl QueryService {
    pub fn new(repo: Arc<dyn DataSourceRepository>, registry: Arc<DriverRegistry>) -> Self {
        Self { repo, registry }
    }

    pub async fn execute(&self, request: QueryExecuteRequest) -> AppResult<QueryExecuteResponse> {
        let datasource = self
            .repo
            .get(&request.datasource_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("datasource {} not found", request.datasource_id))
            })?;

        let driver = self.registry.get(&datasource.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", datasource.db_type))
        })?;

        driver.execute(&datasource, &request).await
    }

    pub async fn cancel(&self, query_id: &str) -> AppResult<QueryCancelResponse> {
        Ok(QueryCancelResponse {
            query_id: query_id.to_string(),
            status: "cancelled".to_string(),
        })
    }
}
