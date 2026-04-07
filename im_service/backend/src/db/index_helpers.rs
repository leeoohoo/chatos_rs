use mongodb::options::IndexOptions;
use mongodb::{Collection, IndexModel};

pub(super) async fn ensure_unique_index(
    collection: Collection<mongodb::bson::Document>,
    keys: mongodb::bson::Document,
) -> Result<(), String> {
    let options = IndexOptions::builder().unique(Some(true)).build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(super) async fn ensure_index(
    collection: Collection<mongodb::bson::Document>,
    keys: mongodb::bson::Document,
) -> Result<(), String> {
    let model = IndexModel::builder().keys(keys).build();
    collection
        .create_index(model)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
