use futures_util::TryStreamExt;
use mongodb::{results::CollectionType, IndexModel};

use crate::{
    domain::{
        datasource::DataSource,
        metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse},
    },
    error::AppResult,
};

use super::{
    super::connection::{connect_client, map_db_error},
    common::{make_db_node, paginate_nodes, parse_collection_node, parse_database_node},
    dbs::list_databases,
};

pub async fn list_nodes(
    datasource: &DataSource,
    parent_id: Option<&str>,
    page: u32,
    page_size: u32,
) -> AppResult<MetadataNodesResponse> {
    let parent = parent_id.unwrap_or("root");

    let items = if parent == "root" {
        list_database_nodes(datasource).await?
    } else if let Some(database) = parse_database_node(parent) {
        list_database_children(datasource, &database).await?
    } else if let Some((database, collection)) = parse_collection_node(parent) {
        list_collection_children(datasource, &database, &collection).await?
    } else {
        Vec::new()
    };

    Ok(paginate_nodes(items, page, page_size))
}

async fn list_database_nodes(datasource: &DataSource) -> AppResult<Vec<MetadataNode>> {
    let databases = list_databases(datasource, None, 1, 10_000).await?.items;
    Ok(databases
        .into_iter()
        .map(|database| make_db_node(&database.name))
        .collect())
}

async fn list_database_children(
    datasource: &DataSource,
    database: &str,
) -> AppResult<Vec<MetadataNode>> {
    let client = connect_client(datasource).await?;
    let db = client.database(database);
    let mut cursor = db
        .list_collections(None, None)
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let mut nodes = Vec::new();

    while let Some(info) = cursor
        .try_next()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
    {
        let name = info.name.to_string();
        if name.is_empty() {
            continue;
        }
        let (id, node_type, has_children) = match info.collection_type {
            CollectionType::View => (
                format!("view:{database}:{name}"),
                MetadataNodeType::View,
                false,
            ),
            CollectionType::Collection | CollectionType::Timeseries => (
                format!("collection:{database}:{name}"),
                MetadataNodeType::Collection,
                true,
            ),
            _ => (
                format!("collection:{database}:{name}"),
                MetadataNodeType::Collection,
                true,
            ),
        };

        nodes.push(MetadataNode {
            id,
            parent_id: format!("db:{database}"),
            node_type,
            display_name: name.clone(),
            path: format!("{database}.{name}"),
            has_children,
        });
    }

    nodes.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(nodes)
}

async fn list_collection_children(
    datasource: &DataSource,
    database: &str,
    collection: &str,
) -> AppResult<Vec<MetadataNode>> {
    let client = connect_client(datasource).await?;
    let db = client.database(database);
    let collection_ref = db.collection::<mongodb::bson::Document>(collection);
    let mut cursor = collection_ref
        .list_indexes(None)
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let mut nodes = Vec::new();

    while let Some(index) = cursor
        .try_next()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
    {
        let name = index_name(&index);
        nodes.push(MetadataNode {
            id: format!("index:{database}:{collection}:{name}"),
            parent_id: format!("collection:{database}:{collection}"),
            node_type: MetadataNodeType::Index,
            display_name: name.clone(),
            path: format!("{database}.{collection}.{name}"),
            has_children: false,
        });
    }

    nodes.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(nodes)
}

fn index_name(index: &IndexModel) -> String {
    if let Some(name) = index
        .options
        .as_ref()
        .and_then(|options| options.name.clone())
    {
        return name;
    }

    let keys = index.keys.keys().cloned().collect::<Vec<_>>();
    if keys.is_empty() {
        "index".to_string()
    } else {
        keys.join("_")
    }
}
