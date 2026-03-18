use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{Contact, CreateContactRequest};

use super::now_rfc3339;

fn collection(db: &Db) -> mongodb::Collection<Contact> {
    db.collection::<Contact>("contacts")
}

fn is_duplicate_key_error(err: &mongodb::error::Error) -> bool {
    err.to_string().contains("E11000")
}

pub async fn list_contacts(
    db: &Db,
    user_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Contact>, String> {
    let mut filter = doc! {
        "user_id": user_id,
    };
    if let Some(value) = status {
        filter.insert("status", value);
    }

    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn get_contact_by_id(db: &Db, contact_id: &str) -> Result<Option<Contact>, String> {
    collection(db)
        .find_one(doc! {"id": contact_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_contact_by_user_and_agent(
    db: &Db,
    user_id: &str,
    agent_id: &str,
) -> Result<Option<Contact>, String> {
    collection(db)
        .find_one(doc! {
            "user_id": user_id,
            "agent_id": agent_id,
        })
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_contacts_by_ids(
    db: &Db,
    user_id: &str,
    contact_ids: &[String],
    status: Option<&str>,
) -> Result<Vec<Contact>, String> {
    if contact_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut filter = doc! {
        "user_id": user_id,
        "id": { "$in": contact_ids.to_vec() },
    };
    if let Some(value) = status {
        filter.insert("status", value);
    }

    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1})
        .build();
    let cursor = collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn create_contact_idempotent(
    db: &Db,
    req: CreateContactRequest,
) -> Result<(Contact, bool), String> {
    if let Some(existing) =
        get_contact_by_user_and_agent(db, req.user_id.as_str(), req.agent_id.as_str()).await?
    {
        return Ok((existing, false));
    }

    let now = now_rfc3339();
    let contact = Contact {
        id: Uuid::new_v4().to_string(),
        user_id: req.user_id,
        agent_id: req.agent_id,
        agent_name_snapshot: req.agent_name_snapshot,
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };

    match collection(db).insert_one(contact.clone()).await {
        Ok(_) => Ok((contact, true)),
        Err(err) => {
            if is_duplicate_key_error(&err) {
                if let Some(existing) = get_contact_by_user_and_agent(
                    db,
                    contact.user_id.as_str(),
                    contact.agent_id.as_str(),
                )
                .await?
                {
                    return Ok((existing, false));
                }
            }
            Err(err.to_string())
        }
    }
}

pub async fn delete_contact_by_id(db: &Db, contact_id: &str) -> Result<bool, String> {
    let result = collection(db)
        .delete_one(doc! {"id": contact_id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}
