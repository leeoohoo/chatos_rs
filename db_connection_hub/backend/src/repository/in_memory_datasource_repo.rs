use crate::{
    domain::datasource::DataSource,
    error::{AppError, AppResult},
    repository::datasource_repo::DataSourceRepository,
};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

#[derive(Default)]
pub struct InMemoryDataSourceRepository {
    store: RwLock<HashMap<String, DataSource>>,
}

#[async_trait]
impl DataSourceRepository for InMemoryDataSourceRepository {
    async fn create(&self, datasource: DataSource) -> AppResult<()> {
        let mut store = self.store.write().await;
        if store.contains_key(&datasource.id) {
            return Err(AppError::Conflict(format!(
                "datasource {} already exists",
                datasource.id
            )));
        }

        store.insert(datasource.id.clone(), datasource);
        Ok(())
    }

    async fn update(&self, datasource: DataSource) -> AppResult<()> {
        let mut store = self.store.write().await;
        if !store.contains_key(&datasource.id) {
            return Err(AppError::NotFound(format!(
                "datasource {} not found",
                datasource.id
            )));
        }

        store.insert(datasource.id.clone(), datasource);
        Ok(())
    }

    async fn delete(&self, id: &str) -> AppResult<()> {
        let mut store = self.store.write().await;
        if store.remove(id).is_none() {
            return Err(AppError::NotFound(format!("datasource {id} not found")));
        }
        Ok(())
    }

    async fn get(&self, id: &str) -> AppResult<Option<DataSource>> {
        let store = self.store.read().await;
        Ok(store.get(id).cloned())
    }

    async fn list(&self) -> AppResult<Vec<DataSource>> {
        let store = self.store.read().await;
        Ok(store.values().cloned().collect())
    }
}
