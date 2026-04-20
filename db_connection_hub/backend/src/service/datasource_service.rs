use crate::{
    domain::datasource::{
        ConnectionStatus, ConnectionTestResult, DataSource, DataSourceCreateRequest,
        DataSourceHealthResponse, DataSourceListItem, DataSourceListResponse,
        DataSourceMutationResponse, DataSourceOptions, DataSourceUpdateRequest,
        DatabaseListResponse, DatabaseSummaryResponse,
    },
    domain::metadata::ObjectStatsResponse,
    drivers::registry::DriverRegistry,
    error::{AppError, AppResult},
    repository::datasource_repo::DataSourceRepository,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

pub struct DataSourceService {
    repo: Arc<dyn DataSourceRepository>,
    registry: Arc<DriverRegistry>,
}

impl DataSourceService {
    pub fn new(repo: Arc<dyn DataSourceRepository>, registry: Arc<DriverRegistry>) -> Self {
        Self { repo, registry }
    }

    pub async fn create(
        &self,
        request: DataSourceCreateRequest,
    ) -> AppResult<DataSourceMutationResponse> {
        let driver = self.registry.get(&request.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", request.db_type))
        })?;

        let descriptor = driver.descriptor();
        if !descriptor.auth_modes.contains(&request.auth.mode) {
            return Err(AppError::BadRequest(format!(
                "auth mode {:?} is not supported for {}",
                request.auth.mode, descriptor.label
            )));
        }

        if !descriptor.network_modes.contains(&request.network.mode) {
            return Err(AppError::BadRequest(format!(
                "network mode {:?} is not supported for {}",
                request.network.mode, descriptor.label
            )));
        }

        let now = Utc::now();
        let datasource = DataSource {
            id: format!("ds_{}", Uuid::new_v4().simple()),
            name: request.name,
            db_type: request.db_type,
            network: request.network,
            auth: request.auth,
            tls: request.tls,
            options: DataSourceOptions::with_defaults(request.options),
            tags: request.tags.unwrap_or_default(),
            status: ConnectionStatus::Unknown,
            created_at: now,
            updated_at: now,
            last_test: None,
        };

        self.repo.create(datasource.clone()).await?;

        Ok(DataSourceMutationResponse {
            id: datasource.id,
            status: "created".to_string(),
        })
    }

    pub async fn list(&self) -> AppResult<DataSourceListResponse> {
        let items = self.repo.list().await?;
        let total = items.len() as u64;

        let mapped = items
            .into_iter()
            .map(|item| DataSourceListItem {
                id: item.id,
                name: item.name,
                db_type: item.db_type,
                status: item.status,
            })
            .collect();

        Ok(DataSourceListResponse {
            items: mapped,
            total,
        })
    }

    pub async fn detail(&self, id: &str) -> AppResult<DataSource> {
        self.require_datasource(id).await
    }

    pub async fn update(
        &self,
        id: &str,
        request: DataSourceUpdateRequest,
    ) -> AppResult<DataSourceMutationResponse> {
        let mut datasource = self.require_datasource(id).await?;

        if let Some(name) = request.name {
            datasource.name = name;
        }
        if let Some(network) = request.network {
            datasource.network = network;
        }
        if let Some(auth) = request.auth {
            datasource.auth = auth;
        }
        if request.tls.is_some() {
            datasource.tls = request.tls;
        }
        if let Some(options) = request.options {
            datasource.options = DataSourceOptions::with_defaults(Some(options));
        }
        if let Some(tags) = request.tags {
            datasource.tags = tags;
        }

        datasource.updated_at = Utc::now();
        self.repo.update(datasource.clone()).await?;

        Ok(DataSourceMutationResponse {
            id: datasource.id,
            status: "updated".to_string(),
        })
    }

    pub async fn delete(&self, id: &str) -> AppResult<DataSourceMutationResponse> {
        self.repo.delete(id).await?;

        Ok(DataSourceMutationResponse {
            id: id.to_string(),
            status: "deleted".to_string(),
        })
    }

    pub async fn test_connection(
        &self,
        id: &str,
    ) -> AppResult<crate::domain::datasource::ConnectionTestResult> {
        let mut datasource = self.require_datasource(id).await?;
        let driver = self.registry.get(&datasource.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", datasource.db_type))
        })?;

        let test_result = match driver.test_connection(&datasource).await {
            Ok(result) => result,
            Err(err) => build_failed_test_result(&datasource, &err),
        };
        datasource.status = if test_result.ok {
            ConnectionStatus::Online
        } else {
            ConnectionStatus::Offline
        };
        datasource.last_test = Some(test_result.clone());
        datasource.updated_at = Utc::now();

        self.repo.update(datasource).await?;

        Ok(test_result)
    }

    pub async fn health(&self, id: &str) -> AppResult<DataSourceHealthResponse> {
        let datasource = self.require_datasource(id).await?;

        Ok(DataSourceHealthResponse {
            status: datasource.status,
            last_test_at: datasource.last_test.as_ref().map(|_| datasource.updated_at),
            last_latency_ms: datasource
                .last_test
                .as_ref()
                .map(|result| result.latency_ms),
            failed_count_1h: datasource
                .last_test
                .as_ref()
                .map(|result| if result.ok { 0 } else { 1 })
                .unwrap_or(0),
        })
    }

    pub async fn database_summary(&self, id: &str) -> AppResult<DatabaseSummaryResponse> {
        let datasource = self.require_datasource(id).await?;
        let driver = self.registry.get(&datasource.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", datasource.db_type))
        })?;

        driver.database_summary(&datasource).await
    }

    pub async fn list_databases(
        &self,
        id: &str,
        keyword: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> AppResult<DatabaseListResponse> {
        let datasource = self.require_datasource(id).await?;
        let driver = self.registry.get(&datasource.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", datasource.db_type))
        })?;

        driver
            .list_databases(&datasource, keyword, page.max(1), page_size.clamp(1, 500))
            .await
    }

    pub async fn object_stats(&self, id: &str, database: &str) -> AppResult<ObjectStatsResponse> {
        let datasource = self.require_datasource(id).await?;
        let driver = self.registry.get(&datasource.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", datasource.db_type))
        })?;

        driver.object_stats(&datasource, database).await
    }

    pub async fn discover_databases(
        &self,
        request: DataSourceCreateRequest,
        keyword: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> AppResult<DatabaseListResponse> {
        let driver = self.registry.get(&request.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", request.db_type))
        })?;

        let descriptor = driver.descriptor();
        if !descriptor.auth_modes.contains(&request.auth.mode) {
            return Err(AppError::BadRequest(format!(
                "auth mode {:?} is not supported for {}",
                request.auth.mode, descriptor.label
            )));
        }

        if !descriptor.network_modes.contains(&request.network.mode) {
            return Err(AppError::BadRequest(format!(
                "network mode {:?} is not supported for {}",
                request.network.mode, descriptor.label
            )));
        }

        let now = Utc::now();
        let transient_datasource = DataSource {
            id: format!("preview_{}", Uuid::new_v4().simple()),
            name: request.name,
            db_type: request.db_type,
            network: request.network,
            auth: request.auth,
            tls: request.tls,
            options: DataSourceOptions::with_defaults(request.options),
            tags: request.tags.unwrap_or_default(),
            status: ConnectionStatus::Unknown,
            created_at: now,
            updated_at: now,
            last_test: None,
        };

        driver
            .list_databases(
                &transient_datasource,
                keyword,
                page.max(1),
                page_size.clamp(1, 500),
            )
            .await
    }

    pub async fn test_connection_preview(
        &self,
        request: DataSourceCreateRequest,
    ) -> AppResult<crate::domain::datasource::ConnectionTestResult> {
        let driver = self.registry.get(&request.db_type).ok_or_else(|| {
            AppError::BadRequest(format!("unsupported db_type {}", request.db_type))
        })?;

        let descriptor = driver.descriptor();
        if !descriptor.auth_modes.contains(&request.auth.mode) {
            return Err(AppError::BadRequest(format!(
                "auth mode {:?} is not supported for {}",
                request.auth.mode, descriptor.label
            )));
        }

        if !descriptor.network_modes.contains(&request.network.mode) {
            return Err(AppError::BadRequest(format!(
                "network mode {:?} is not supported for {}",
                request.network.mode, descriptor.label
            )));
        }

        let now = Utc::now();
        let transient_datasource = DataSource {
            id: format!("preview_{}", Uuid::new_v4().simple()),
            name: request.name,
            db_type: request.db_type,
            network: request.network,
            auth: request.auth,
            tls: request.tls,
            options: DataSourceOptions::with_defaults(request.options),
            tags: request.tags.unwrap_or_default(),
            status: ConnectionStatus::Unknown,
            created_at: now,
            updated_at: now,
            last_test: None,
        };

        let test_result = match driver.test_connection(&transient_datasource).await {
            Ok(result) => result,
            Err(err) => build_failed_test_result(&transient_datasource, &err),
        };

        Ok(test_result)
    }

    pub async fn require_datasource(&self, id: &str) -> AppResult<DataSource> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("datasource {id} not found")))
    }
}

fn build_failed_test_result(datasource: &DataSource, err: &AppError) -> ConnectionTestResult {
    let message = err.to_string();
    let error_code = if message.contains("CONN_AUTH_FAILED") {
        "CONN_AUTH_FAILED"
    } else if message.contains("CONN_DB_NOT_FOUND") {
        "CONN_DB_NOT_FOUND"
    } else if message.contains("CONN_TIMEOUT") {
        "CONN_TIMEOUT"
    } else if message.contains("CONN_TLS_HANDSHAKE_FAILED") {
        "CONN_TLS_HANDSHAKE_FAILED"
    } else {
        "CONN_NETWORK_UNREACHABLE"
    };

    let stage = if error_code == "CONN_AUTH_FAILED" {
        "auth"
    } else if error_code == "CONN_TLS_HANDSHAKE_FAILED" {
        "tls"
    } else {
        "network"
    };

    ConnectionTestResult {
        ok: false,
        latency_ms: 0,
        server_version: None,
        auth_mode: datasource.auth.mode,
        checks: Vec::new(),
        error_code: Some(error_code.to_string()),
        message: Some(message),
        stage: Some(stage.to_string()),
    }
}
