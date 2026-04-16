use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};

use crate::{
    domain::{
        datasource::DataSource,
        metadata::{
            MetadataNodeType, ObjectColumn, ObjectConstraint, ObjectDetailResponse, ObjectIndex,
        },
    },
    error::{AppError, AppResult},
};

use super::{
    super::connection::{connect_client, map_db_error},
    common::{parse_detail_node, parse_index_node},
};

pub async fn object_detail(
    datasource: &DataSource,
    node_id: &str,
) -> AppResult<ObjectDetailResponse> {
    if let Some((node_type, database, object_name)) = parse_detail_node(node_id) {
        return load_collection_or_view_detail(
            datasource,
            node_id,
            node_type,
            &database,
            &object_name,
        )
        .await;
    }

    if let Some((database, collection_name, index_name)) = parse_index_node(node_id) {
        return load_index_detail(
            datasource,
            node_id,
            &database,
            &collection_name,
            &index_name,
        )
        .await;
    }

    Err(AppError::NotFound(format!(
        "unsupported node for detail: {node_id}"
    )))
}

async fn load_collection_or_view_detail(
    datasource: &DataSource,
    node_id: &str,
    node_type: MetadataNodeType,
    database: &str,
    object_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let client = connect_client(datasource).await?;
    let db = client.database(database);
    let collection = db.collection::<Document>(object_name);

    let sample = collection
        .find_one(doc! {}, None)
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let columns = build_columns(sample.as_ref());
    let indexes = if matches!(node_type, MetadataNodeType::Collection) {
        list_indexes(&collection).await?
    } else {
        Vec::new()
    };

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type,
        name: object_name.to_string(),
        columns,
        indexes,
        constraints: Vec::<ObjectConstraint>::new(),
        ddl: None,
    })
}

async fn load_index_detail(
    datasource: &DataSource,
    node_id: &str,
    database: &str,
    collection_name: &str,
    index_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let client = connect_client(datasource).await?;
    let db = client.database(database);
    let collection = db.collection::<Document>(collection_name);
    let mut cursor = collection
        .list_indexes(None)
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    while let Some(index) = cursor
        .try_next()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
    {
        let name = index
            .options
            .as_ref()
            .and_then(|options| options.name.clone())
            .unwrap_or_else(|| "index".to_string());

        if name != index_name {
            continue;
        }

        let columns = index.keys.keys().cloned().collect::<Vec<_>>();
        let is_unique = index
            .options
            .as_ref()
            .and_then(|options| options.unique)
            .unwrap_or(false);

        return Ok(ObjectDetailResponse {
            node_id: node_id.to_string(),
            node_type: MetadataNodeType::Index,
            name: index_name.to_string(),
            columns: Vec::new(),
            indexes: vec![ObjectIndex {
                name: index_name.to_string(),
                columns,
                is_unique,
            }],
            constraints: Vec::new(),
            ddl: None,
        });
    }

    Err(AppError::NotFound(format!(
        "mongodb index not found: {database}.{collection_name}.{index_name}"
    )))
}

fn build_columns(sample: Option<&Document>) -> Vec<ObjectColumn> {
    match sample {
        Some(document) => document
            .iter()
            .map(|(name, value)| ObjectColumn {
                name: name.to_string(),
                data_type: bson_type_name(value),
                nullable: true,
            })
            .collect(),
        None => Vec::new(),
    }
}

async fn list_indexes(collection: &mongodb::Collection<Document>) -> AppResult<Vec<ObjectIndex>> {
    let mut cursor = collection
        .list_indexes(None)
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let mut items = Vec::new();
    while let Some(index) = cursor
        .try_next()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
    {
        let name = index
            .options
            .as_ref()
            .and_then(|options| options.name.clone())
            .unwrap_or_else(|| "index".to_string());
        let columns = index.keys.keys().cloned().collect::<Vec<_>>();
        let is_unique = index
            .options
            .as_ref()
            .and_then(|options| options.unique)
            .unwrap_or(false);

        items.push(ObjectIndex {
            name,
            columns,
            is_unique,
        });
    }

    Ok(items)
}

fn bson_type_name(value: &Bson) -> String {
    match value {
        Bson::Double(_) => "double".to_string(),
        Bson::String(_) => "string".to_string(),
        Bson::Array(_) => "array".to_string(),
        Bson::Document(_) => "object".to_string(),
        Bson::Boolean(_) => "bool".to_string(),
        Bson::Null => "null".to_string(),
        Bson::RegularExpression(_) => "regex".to_string(),
        Bson::JavaScriptCode(_) => "javascript".to_string(),
        Bson::JavaScriptCodeWithScope(_) => "javascript_scope".to_string(),
        Bson::Int32(_) => "int32".to_string(),
        Bson::Int64(_) => "int64".to_string(),
        Bson::Timestamp(_) => "timestamp".to_string(),
        Bson::Binary(_) => "binary".to_string(),
        Bson::ObjectId(_) => "object_id".to_string(),
        Bson::DateTime(_) => "datetime".to_string(),
        Bson::Symbol(_) => "symbol".to_string(),
        Bson::Decimal128(_) => "decimal128".to_string(),
        Bson::Undefined => "undefined".to_string(),
        Bson::MaxKey => "max_key".to_string(),
        Bson::MinKey => "min_key".to_string(),
        Bson::DbPointer(_) => "db_pointer".to_string(),
    }
}
