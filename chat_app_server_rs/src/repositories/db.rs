use std::sync::Arc;
use mongodb::bson::{Document, Bson};

use crate::db::{self, Database};

pub async fn get_db() -> Result<Arc<Database>, String> {
    db::get_db().await
}

pub fn get_db_sync() -> Result<Arc<Database>, String> {
    db::get_db_sync()
}

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

pub async fn with_db<T, Fmongo, Fsqlite>(mongo_fn: Fmongo, sqlite_fn: Fsqlite) -> Result<T, String>
where
    Fmongo: for<'a> FnOnce(&'a mongodb::Database) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send + 'a>>,
    Fsqlite: for<'a> FnOnce(&'a sqlx::SqlitePool) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send + 'a>>,
{
    let db = get_db().await?;
    match db.as_ref() {
        Database::Mongo { db, .. } => mongo_fn(db).await,
        Database::Sqlite(pool) => sqlite_fn(pool).await,
    }
}

