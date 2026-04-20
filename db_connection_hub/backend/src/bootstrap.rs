use crate::{
    drivers::registry::DriverRegistry,
    repository::{
        datasource_repo::DataSourceRepository, sqlite_datasource_repo::SqliteDataSourceRepository,
    },
    service::{
        datasource_service::DataSourceService, meta_service::MetaService,
        metadata_service::MetadataService, query_service::QueryService,
    },
    state::AppState,
};
use std::{io, sync::Arc};

pub async fn build_app_state() -> io::Result<AppState> {
    let store_path = std::env::var("DB_HUB_STORE_PATH")
        .unwrap_or_else(|_| "./data/datasource_store.sqlite3".to_string());
    let repo_impl = SqliteDataSourceRepository::new(store_path)
        .await
        .map_err(|err| io::Error::other(format!("failed to initialize datasource store: {err}")))?;
    let repo: Arc<dyn DataSourceRepository> = Arc::new(repo_impl);

    let registry = Arc::new(DriverRegistry::with_default_drivers());

    let meta_service = Arc::new(MetaService::new(registry.clone()));
    let datasource_service = Arc::new(DataSourceService::new(repo.clone(), registry.clone()));
    let metadata_service = Arc::new(MetadataService::new(repo.clone(), registry.clone()));
    let query_service = Arc::new(QueryService::new(repo, registry));

    Ok(AppState {
        meta_service,
        datasource_service,
        metadata_service,
        query_service,
    })
}
