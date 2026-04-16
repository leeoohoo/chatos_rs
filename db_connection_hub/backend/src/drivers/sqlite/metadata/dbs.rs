use std::fs;

use crate::{
    domain::datasource::{DataSource, DatabaseInfo, DatabaseListResponse, DatabaseSummaryResponse},
    error::{AppError, AppResult},
};

pub async fn database_summary(_datasource: &DataSource) -> AppResult<DatabaseSummaryResponse> {
    Ok(DatabaseSummaryResponse {
        database_count: 1,
        visible_database_count: 1,
        visibility_scope: "full".to_string(),
    })
}

pub async fn list_databases(
    datasource: &DataSource,
    keyword: Option<&str>,
    page: u32,
    page_size: u32,
) -> AppResult<DatabaseListResponse> {
    let name = "main".to_string();
    if let Some(keyword) = keyword {
        if !name.contains(keyword) {
            return Ok(DatabaseListResponse {
                items: Vec::new(),
                page,
                page_size,
                total: 0,
            });
        }
    }

    let file_path = datasource.network.file_path.clone().ok_or_else(|| {
        AppError::BadRequest("network.file_path is required for sqlite".to_string())
    })?;

    let size = fs::metadata(file_path).ok().map(|meta| meta.len());

    Ok(DatabaseListResponse {
        items: vec![DatabaseInfo {
            name,
            owner: None,
            size_bytes: size,
        }],
        page: page.max(1),
        page_size: page_size.clamp(1, 500),
        total: 1,
    })
}
