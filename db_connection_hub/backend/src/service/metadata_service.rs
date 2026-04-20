use crate::{
    domain::metadata::{MetadataNodesResponse, ObjectDetailResponse},
    drivers::registry::DriverRegistry,
    error::{AppError, AppResult},
    repository::datasource_repo::DataSourceRepository,
};
use std::sync::Arc;

pub struct MetadataService {
    repo: Arc<dyn DataSourceRepository>,
    registry: Arc<DriverRegistry>,
}

impl MetadataService {
    pub fn new(repo: Arc<dyn DataSourceRepository>, registry: Arc<DriverRegistry>) -> Self {
        Self { repo, registry }
    }

    pub async fn list_nodes(
        &self,
        datasource_id: &str,
        parent_id: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> AppResult<MetadataNodesResponse> {
        let datasource =
            self.repo.get(datasource_id).await?.ok_or_else(|| {
                AppError::NotFound(format!("datasource {datasource_id} not found"))
            })?;

        let driver = self.registry.get(&datasource.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", datasource.db_type))
        })?;

        driver
            .list_nodes(&datasource, parent_id, page.max(1), page_size.clamp(1, 500))
            .await
    }

    pub async fn object_detail(
        &self,
        datasource_id: &str,
        node_id: &str,
    ) -> AppResult<ObjectDetailResponse> {
        let datasource =
            self.repo.get(datasource_id).await?.ok_or_else(|| {
                AppError::NotFound(format!("datasource {datasource_id} not found"))
            })?;

        let driver = self.registry.get(&datasource.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", datasource.db_type))
        })?;

        driver.object_detail(&datasource, node_id).await
    }
}
