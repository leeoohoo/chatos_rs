use mongodb::options::IndexOptions;
use mongodb::{Collection, IndexModel};
use tracing::info;

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

pub(super) async fn ensure_unique_partial_index(
    collection: Collection<mongodb::bson::Document>,
    keys: mongodb::bson::Document,
    partial_filter_expression: mongodb::bson::Document,
) -> Result<(), String> {
    let options = IndexOptions::builder()
        .unique(Some(true))
        .partial_filter_expression(Some(partial_filter_expression))
        .build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    match collection.create_index(model).await {
        Ok(_) => {}
        Err(err) => {
            let lowered = err.to_string().to_ascii_lowercase();
            if lowered.contains("e11000") || lowered.contains("duplicate key") {
                info!(
                    "[MEMORY-SERVER] skip partial unique index due duplicate legacy data: {}",
                    err
                );
                return Ok(());
            }
            return Err(err.to_string());
        }
    }
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
