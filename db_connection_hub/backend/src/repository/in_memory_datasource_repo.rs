// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::domain::{
        datasource::{
            AuthConfig, ConnectionStatus, DataSource, DataSourceOptions, NetworkConfig,
        },
        meta::{AuthMode, DbType, NetworkMode},
    };

    use super::InMemoryDataSourceRepository;
    use crate::{error::AppError, repository::datasource_repo::DataSourceRepository};

    fn sample_datasource(id: &str) -> DataSource {
        DataSource {
            id: id.to_string(),
            name: format!("Datasource {id}"),
            db_type: DbType::Postgres,
            network: NetworkConfig {
                mode: NetworkMode::Direct,
                host: Some("127.0.0.1".to_string()),
                port: Some(5432),
                database: Some("orders".to_string()),
                service_name: None,
                sid: None,
                file_path: None,
                ssh: None,
            },
            auth: AuthConfig {
                mode: AuthMode::Password,
                username: Some("tester".to_string()),
                password: Some("secret".to_string()),
                access_token: None,
                client_cert: None,
                client_key: None,
                key_ref: None,
                wallet_ref: None,
                principal: None,
                realm: None,
                kdc: None,
                service_name: None,
            },
            tls: None,
            options: DataSourceOptions::with_defaults(None),
            tags: vec!["test".to_string()],
            status: ConnectionStatus::Unknown,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_test: None,
        }
    }

    #[tokio::test]
    async fn create_then_get_returns_datasource() {
        let repo = InMemoryDataSourceRepository::default();
        let datasource = sample_datasource("ds_1");

        repo.create(datasource.clone())
            .await
            .expect("create should succeed");

        let loaded = repo
            .get("ds_1")
            .await
            .expect("get should succeed")
            .expect("datasource should exist");
        assert_eq!(loaded.id, datasource.id);
        assert_eq!(loaded.name, datasource.name);
    }

    #[tokio::test]
    async fn duplicate_create_returns_conflict() {
        let repo = InMemoryDataSourceRepository::default();
        let datasource = sample_datasource("ds_1");

        repo.create(datasource.clone())
            .await
            .expect("first create should succeed");
        let err = repo
            .create(datasource)
            .await
            .expect_err("second create should fail");

        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[tokio::test]
    async fn update_missing_returns_not_found() {
        let repo = InMemoryDataSourceRepository::default();

        let err = repo
            .update(sample_datasource("missing"))
            .await
            .expect_err("update should fail");

        assert!(matches!(err, AppError::NotFound(_)));
    }
}
