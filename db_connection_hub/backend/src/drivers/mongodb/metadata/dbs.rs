use crate::{
    domain::datasource::{DataSource, DatabaseInfo, DatabaseListResponse, DatabaseSummaryResponse},
    error::AppResult,
};

use super::super::connection::{connect_client, map_db_error};

pub async fn database_summary(datasource: &DataSource) -> AppResult<DatabaseSummaryResponse> {
    let client = connect_client(datasource).await?;
    let databases = client
        .list_database_names(None, None)
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let count = databases.len() as u64;

    Ok(DatabaseSummaryResponse {
        database_count: count,
        visible_database_count: count,
        visibility_scope: "full".to_string(),
    })
}

pub async fn list_databases(
    datasource: &DataSource,
    keyword: Option<&str>,
    page: u32,
    page_size: u32,
) -> AppResult<DatabaseListResponse> {
    let client = connect_client(datasource).await?;
    let mut names = client
        .list_database_names(None, None)
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    names.sort();

    if let Some(keyword) = keyword {
        let lowered = keyword.to_lowercase();
        names.retain(|name| name.to_lowercase().contains(&lowered));
    }

    let safe_page = page.max(1);
    let safe_size = page_size.clamp(1, 500);
    let total = names.len() as u64;
    let start = ((safe_page - 1) * safe_size) as usize;

    let items = if start >= names.len() {
        Vec::new()
    } else {
        let end = (start + safe_size as usize).min(names.len());
        names[start..end]
            .iter()
            .map(|name| DatabaseInfo {
                name: name.to_string(),
                owner: None,
                size_bytes: None,
            })
            .collect::<Vec<_>>()
    };

    Ok(DatabaseListResponse {
        items,
        page: safe_page,
        page_size: safe_size,
        total,
    })
}
