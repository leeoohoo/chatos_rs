use futures_util::TryStreamExt;
use mongodb::Cursor;

use crate::db::Db;
use crate::models::EngineThread;

pub(crate) fn thread_collection(db: &Db) -> mongodb::Collection<EngineThread> {
    db.collection::<EngineThread>("engine_threads")
}

pub(crate) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(crate) async fn collect_threads(
    cursor: Cursor<EngineThread>,
) -> Result<Vec<EngineThread>, String> {
    cursor.try_collect().await.map_err(|err| err.to_string())
}
