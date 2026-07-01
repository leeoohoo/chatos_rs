// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::service::{
    datasource_service::DataSourceService, meta_service::MetaService,
    metadata_service::MetadataService, query_service::QueryService,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub meta_service: Arc<MetaService>,
    pub datasource_service: Arc<DataSourceService>,
    pub metadata_service: Arc<MetadataService>,
    pub query_service: Arc<QueryService>,
}
