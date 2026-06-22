use futures_util::TryStreamExt;
use mongodb::{bson::Document, options::IndexOptions, Collection, IndexModel};

fn unique_flag(model: &IndexModel) -> bool {
    model
        .options
        .as_ref()
        .and_then(|options| options.unique)
        .unwrap_or(false)
}

fn has_equivalent_index(indexes: &[IndexModel], keys: &Document, unique: bool) -> bool {
    indexes
        .iter()
        .any(|index| index.keys == *keys && unique_flag(index) == unique)
}

async fn load_indexes(collection: &Collection<Document>) -> Result<Vec<IndexModel>, String> {
    let cursor = match collection.list_indexes().await {
        Ok(cursor) => cursor,
        Err(err) if is_namespace_not_found(&err) => return Ok(Vec::new()),
        Err(err) => return Err(err.to_string()),
    };
    cursor.try_collect().await.map_err(|err| err.to_string())
}

fn is_namespace_not_found(err: &mongodb::error::Error) -> bool {
    let message = err.to_string();
    message.contains("NamespaceNotFound") || message.contains("Error code 26")
}

pub async fn ensure_index(collection: Collection<Document>, keys: Document) -> Result<(), String> {
    if has_equivalent_index(&load_indexes(&collection).await?, &keys, false) {
        return Ok(());
    }
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
    if has_equivalent_index(&load_indexes(&collection).await?, &keys, true) {
        return Ok(());
    }
    let options = IndexOptions::builder().unique(true).build();
    collection
        .create_index(IndexModel::builder().keys(keys).options(options).build())
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub async fn ensure_named_unique_index(
    collection: Collection<Document>,
    name: &str,
    keys: Document,
) -> Result<(), String> {
    if has_equivalent_index(&load_indexes(&collection).await?, &keys, true) {
        return Ok(());
    }
    let options = IndexOptions::builder()
        .name(name.to_string())
        .unique(true)
        .build();
    collection
        .create_index(IndexModel::builder().keys(keys).options(options).build())
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub async fn ensure_named_index(
    collection: Collection<Document>,
    name: &str,
    keys: Document,
) -> Result<(), String> {
    if has_equivalent_index(&load_indexes(&collection).await?, &keys, false) {
        return Ok(());
    }
    let options = IndexOptions::builder().name(name.to_string()).build();
    collection
        .create_index(IndexModel::builder().keys(keys).options(options).build())
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub async fn drop_index_if_exists(
    collection: Collection<Document>,
    name: &str,
) -> Result<(), String> {
    let names = match collection.list_index_names().await {
        Ok(names) => names,
        Err(err) if is_namespace_not_found(&err) => return Ok(()),
        Err(err) => return Err(err.to_string()),
    };
    if names.iter().any(|item| item == name) {
        collection
            .drop_index(name)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{has_equivalent_index, IndexModel, IndexOptions};

    #[test]
    fn equivalent_index_check_ignores_name_when_keys_and_unique_match() {
        let indexes = vec![IndexModel::builder()
            .keys(doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "snapshot_type": 1, "turn_id": 1})
            .options(
                IndexOptions::builder()
                    .name("tenant_id_1_source_id_1_thread_id_1_snapshot_type_1_turn_id_1".to_string())
                    .unique(true)
                    .build(),
            )
            .build()];

        assert!(has_equivalent_index(
            &indexes,
            &doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "snapshot_type": 1, "turn_id": 1},
            true,
        ));
    }

    #[test]
    fn equivalent_index_check_rejects_unique_mismatch() {
        let indexes = vec![IndexModel::builder()
            .keys(doc! {"tenant_id": 1, "source_id": 1})
            .options(IndexOptions::builder().unique(false).build())
            .build()];

        assert!(!has_equivalent_index(
            &indexes,
            &doc! {"tenant_id": 1, "source_id": 1},
            true,
        ));
    }
}
