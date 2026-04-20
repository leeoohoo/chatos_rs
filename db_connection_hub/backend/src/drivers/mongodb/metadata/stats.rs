use futures_util::TryStreamExt;
use mongodb::{bson::Document, results::CollectionType};

use crate::{
    domain::{datasource::DataSource, metadata::ObjectStatsResponse},
    error::AppResult,
};

use super::super::connection::{connect_client, map_db_error};

pub async fn object_stats(
    datasource: &DataSource,
    database: &str,
) -> AppResult<ObjectStatsResponse> {
    let client = connect_client(datasource).await?;
    let db = client.database(database);

    let mut collection_cursor = db
        .list_collections(None, None)
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let mut collection_count: u64 = 0;
    let mut view_count: u64 = 0;
    let mut index_count: u64 = 0;
    let mut partial = false;

    while let Some(info) = collection_cursor
        .try_next()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
    {
        let name = info.name.to_string();
        match info.collection_type {
            CollectionType::View => {
                view_count += 1;
                continue;
            }
            CollectionType::Collection | CollectionType::Timeseries => {
                collection_count += 1;
            }
            _ => {
                collection_count += 1;
            }
        }

        if name.is_empty() {
            continue;
        }

        let collection = db.collection::<Document>(name.as_str());
        match collection.list_indexes(None).await {
            Ok(mut index_cursor) => {
                while let Some(_index) = index_cursor
                    .try_next()
                    .await
                    .map_err(|err| map_db_error("query", err.to_string()))?
                {
                    index_count += 1;
                }
            }
            Err(_err) => {
                partial = true;
            }
        }
    }

    Ok(ObjectStatsResponse {
        database: database.to_string(),
        schema_count: None,
        table_count: None,
        view_count: Some(view_count),
        materialized_view_count: None,
        collection_count: Some(collection_count),
        index_count: Some(index_count),
        procedure_count: None,
        function_count: None,
        trigger_count: None,
        sequence_count: None,
        synonym_count: None,
        package_count: None,
        partial,
    })
}
