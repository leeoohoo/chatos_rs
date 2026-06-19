use mongodb::bson::{Bson, Document, doc};
use mongodb::options::UpdateOptions;
use std::sync::Arc;

use crate::db::{self, Database};

pub async fn get_db() -> Result<Arc<Database>, String> {
    db::get_db().await
}

#[cfg(test)]
pub fn get_db_sync() -> Result<Arc<Database>, String> {
    db::get_db_sync()
}

#[cfg(test)]
pub fn is_mongo(db: &Database) -> bool {
    db.is_mongo()
}

pub fn to_doc(doc: Document) -> Document {
    doc.into_iter()
        .filter(|(_, v)| !matches!(v, Bson::Null))
        .collect()
}

pub fn doc_from_pairs(pairs: Vec<(&str, Bson)>) -> Document {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

pub async fn mongo_find_one_doc(
    db: &mongodb::Database,
    collection_name: &str,
    filter: Document,
) -> Result<Option<Document>, String> {
    db.collection::<Document>(collection_name)
        .find_one(filter, None)
        .await
        .map_err(|e| e.to_string())
}

pub async fn mongo_insert_doc(
    db: &mongodb::Database,
    collection_name: &str,
    doc: Document,
) -> Result<(), String> {
    db.collection::<Document>(collection_name)
        .insert_one(doc, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn mongo_update_set_doc(
    db: &mongodb::Database,
    collection_name: &str,
    filter: Document,
    set_doc: Document,
) -> Result<(), String> {
    mongo_update_one_doc(db, collection_name, filter, doc! { "$set": set_doc }, None).await
}

pub async fn mongo_update_many_set_doc(
    db: &mongodb::Database,
    collection_name: &str,
    filter: Document,
    set_doc: Document,
) -> Result<(), String> {
    db.collection::<Document>(collection_name)
        .update_many(filter, doc! { "$set": set_doc }, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn mongo_update_one_doc(
    db: &mongodb::Database,
    collection_name: &str,
    filter: Document,
    update: Document,
    options: Option<UpdateOptions>,
) -> Result<(), String> {
    db.collection::<Document>(collection_name)
        .update_one(filter, update, options)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn mongo_upsert_set_doc(
    db: &mongodb::Database,
    collection_name: &str,
    filter: Document,
    set_doc: Document,
) -> Result<(), String> {
    mongo_update_one_doc(
        db,
        collection_name,
        filter,
        doc! { "$set": to_doc(set_doc) },
        Some(UpdateOptions::builder().upsert(true).build()),
    )
    .await
}

pub async fn mongo_delete_one_doc(
    db: &mongodb::Database,
    collection_name: &str,
    filter: Document,
) -> Result<(), String> {
    db.collection::<Document>(collection_name)
        .delete_one(filter, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn mongo_delete_many_doc(
    db: &mongodb::Database,
    collection_name: &str,
    filter: Document,
) -> Result<(), String> {
    db.collection::<Document>(collection_name)
        .delete_many(filter, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn with_db<T, Fmongo, Fsqlite>(mongo_fn: Fmongo, sqlite_fn: Fsqlite) -> Result<T, String>
where
    Fmongo: for<'a> FnOnce(
        &'a mongodb::Database,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<T, String>> + Send + 'a>,
    >,
    Fsqlite: for<'a> FnOnce(
        &'a sqlx::SqlitePool,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<T, String>> + Send + 'a>,
    >,
{
    let db = get_db().await?;
    match db.as_ref() {
        Database::Mongo { db, .. } => mongo_fn(db).await,
        Database::Sqlite(pool) => sqlite_fn(pool).await,
    }
}
