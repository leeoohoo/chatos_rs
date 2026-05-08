use mongodb::{
    bson::Document,
    options::IndexOptions,
    Collection,
    IndexModel,
};

pub async fn ensure_index(collection: Collection<Document>, keys: Document) -> Result<(), String> {
    collection
        .create_index(IndexModel::builder().keys(keys).build())
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub async fn ensure_unique_index(
    collection: Collection<Document>,
    keys: Document,
) -> Result<(), String> {
    let options = IndexOptions::builder().unique(true).build();
    collection
        .create_index(IndexModel::builder().keys(keys).options(options).build())
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}
