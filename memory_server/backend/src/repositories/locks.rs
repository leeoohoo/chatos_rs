use mongodb::bson::{doc, DateTime};
use uuid::Uuid;

use crate::db::Db;

use super::now_rfc3339;

#[derive(Debug, Clone)]
pub struct JobLockHandle {
    pub lock_key: String,
    pub owner_token: String,
}

fn collection(db: &Db) -> mongodb::Collection<mongodb::bson::Document> {
    db.collection::<mongodb::bson::Document>("job_locks")
}

fn is_duplicate_key_error(err: &mongodb::error::Error) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("e11000") || text.contains("duplicate key")
}

pub async fn try_acquire_job_lock(
    db: &Db,
    lock_key: &str,
    lease_seconds: i64,
) -> Result<Option<JobLockHandle>, String> {
    let key = lock_key.trim();
    if key.is_empty() {
        return Err("lock_key is empty".to_string());
    }

    let now_ts = chrono::Utc::now().timestamp();
    let lease = lease_seconds.max(60);
    let expires_at_ts = now_ts.saturating_add(lease);
    let owner_token = Uuid::new_v4().to_string();
    let now_str = now_rfc3339();

    let result = collection(db)
        .update_one(
            doc! {
                "lock_key": key,
                "$or": [
                    {"expires_at_ts": {"$lte": now_ts}},
                    {"expires_at_ts": {"$exists": false}},
                ]
            },
            doc! {
                "$set": {
                    "owner_token": owner_token.as_str(),
                    "expires_at_ts": expires_at_ts,
                    "expires_at": DateTime::from_millis(expires_at_ts * 1000),
                    "updated_at": now_str.as_str(),
                },
                "$setOnInsert": {
                    "lock_key": key,
                    "created_at": now_str.as_str(),
                }
            },
        )
        .upsert(true)
        .await;

    match result {
        Ok(_) => Ok(Some(JobLockHandle {
            lock_key: key.to_string(),
            owner_token,
        })),
        Err(err) if is_duplicate_key_error(&err) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

pub async fn refresh_job_lock(
    db: &Db,
    handle: &JobLockHandle,
    lease_seconds: i64,
) -> Result<bool, String> {
    let now_ts = chrono::Utc::now().timestamp();
    let expires_at_ts = now_ts.saturating_add(lease_seconds.max(60));
    let now_str = now_rfc3339();

    let result = collection(db)
        .update_one(
            doc! {
                "lock_key": handle.lock_key.as_str(),
                "owner_token": handle.owner_token.as_str(),
            },
            doc! {
                "$set": {
                    "expires_at_ts": expires_at_ts,
                    "expires_at": DateTime::from_millis(expires_at_ts * 1000),
                    "updated_at": now_str.as_str(),
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.modified_count > 0)
}

pub async fn release_job_lock(db: &Db, handle: &JobLockHandle) -> Result<(), String> {
    collection(db)
        .delete_one(doc! {
            "lock_key": handle.lock_key.as_str(),
            "owner_token": handle.owner_token.as_str(),
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
