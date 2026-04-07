use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateImContactRequest, ImContact};

use super::{normalize_optional_text, now_rfc3339};

fn collection(db: &Db) -> mongodb::Collection<ImContact> {
    db.collection::<ImContact>("contacts")
}

pub async fn create_contact(db: &Db, req: CreateImContactRequest) -> Result<ImContact, String> {
    let now = now_rfc3339();
    let contact = ImContact {
        id: Uuid::new_v4().to_string(),
        owner_user_id: req.owner_user_id.trim().to_string(),
        agent_id: req.agent_id.trim().to_string(),
        display_name: req.display_name.trim().to_string(),
        avatar_url: normalize_optional_text(req.avatar_url.as_deref()),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };

    collection(db)
        .insert_one(contact.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(contact)
}

pub async fn get_contact_by_id(db: &Db, contact_id: &str) -> Result<Option<ImContact>, String> {
    collection(db)
        .find_one(doc! {"id": contact_id.trim()})
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_contacts_by_owner(
    db: &Db,
    owner_user_id: &str,
    limit: i64,
) -> Result<Vec<ImContact>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .limit(Some(limit.max(1)))
        .build();

    let cursor = collection(db)
        .find(doc! {"owner_user_id": owner_user_id.trim()})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}
