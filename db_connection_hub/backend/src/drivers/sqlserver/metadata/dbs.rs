use crate::{
    domain::datasource::{DataSource, DatabaseInfo, DatabaseListResponse, DatabaseSummaryResponse},
    error::AppResult,
};

use super::super::connection::{connect_client, map_db_error};

pub async fn database_summary(datasource: &DataSource) -> AppResult<DatabaseSummaryResponse> {
    let mut client = connect_client(datasource, Some("master")).await?;
    let rows = client
        .query(
            "select cast(count(*) as bigint) as cnt from sys.databases where state = 0",
            &[],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let count = rows
        .first()
        .and_then(|row| row.get::<i64, _>(0))
        .unwrap_or(0)
        .max(0) as u64;

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
    let mut client = connect_client(datasource, Some("master")).await?;
    let safe_page = page.max(1);
    let safe_size = page_size.clamp(1, 500);
    let offset = ((safe_page - 1) * safe_size) as i64;
    let page_size_i64 = safe_size as i64;

    let rows = if let Some(keyword) = keyword {
        let like = format!("%{keyword}%");
        client
            .query(
                "select name from sys.databases
                 where state = 0 and name like @P1
                 order by name
                 offset @P2 rows fetch next @P3 rows only",
                &[&like, &offset, &page_size_i64],
            )
            .await
    } else {
        client
            .query(
                "select name from sys.databases
                 where state = 0
                 order by name
                 offset @P1 rows fetch next @P2 rows only",
                &[&offset, &page_size_i64],
            )
            .await
    }
    .map_err(|err| map_db_error("query", err.to_string()))?
    .into_first_result()
    .await
    .map_err(|err| map_db_error("query", err.to_string()))?;

    let total = if let Some(keyword) = keyword {
        let like = format!("%{keyword}%");
        let rows = client
            .query(
                "select cast(count(*) as bigint)
                 from sys.databases
                 where state = 0 and name like @P1",
                &[&like],
            )
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?
            .into_first_result()
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?;

        rows.first()
            .and_then(|row| row.get::<i64, _>(0))
            .unwrap_or(0)
            .max(0) as u64
    } else {
        let rows = client
            .query(
                "select cast(count(*) as bigint)
                 from sys.databases
                 where state = 0",
                &[],
            )
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?
            .into_first_result()
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?;

        rows.first()
            .and_then(|row| row.get::<i64, _>(0))
            .unwrap_or(0)
            .max(0) as u64
    };

    let items = rows
        .into_iter()
        .map(|row| DatabaseInfo {
            name: row.get::<&str, _>(0).unwrap_or_default().to_string(),
            owner: None,
            size_bytes: None,
        })
        .collect();

    Ok(DatabaseListResponse {
        items,
        page: safe_page,
        page_size: safe_size,
        total,
    })
}
