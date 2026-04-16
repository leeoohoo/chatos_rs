use crate::{domain::datasource::DataSource, error::AppResult};
use async_trait::async_trait;

#[async_trait]
pub trait DataSourceRepository: Send + Sync {
    async fn create(&self, datasource: DataSource) -> AppResult<()>;
    async fn update(&self, datasource: DataSource) -> AppResult<()>;
    async fn delete(&self, id: &str) -> AppResult<()>;
    async fn get(&self, id: &str) -> AppResult<Option<DataSource>>;
    async fn list(&self) -> AppResult<Vec<DataSource>>;
}
