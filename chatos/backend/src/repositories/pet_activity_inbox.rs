// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::{doc, from_document, to_bson, Bson, Document};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use uuid::Uuid;

use crate::core::time::now_rfc3339;
use crate::models::pet_activity_inbox::{
    PetActivityInboxRecord, PetActivityInboxStatus, PetActivityInboxUpsert,
};
use crate::repositories::db::with_db;

const COLLECTION: &str = "pet_activity_inbox";

pub async fn list_pet_activities(
    user_id: &str,
    include_closed: bool,
    limit: i64,
) -> Result<Vec<PetActivityInboxRecord>, String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        Box::pin(async move {
            let mut filter = doc! { "user_id": &user_id };
            if !include_closed {
                filter.insert("inbox_status", doc! { "$in": ["unread", "displayed"] });
                filter.insert(
                    "$or",
                    Bson::Array(vec![
                        Bson::Document(doc! { "expires_at": Bson::Null }),
                        Bson::Document(doc! { "expires_at": { "$exists": false } }),
                        Bson::Document(doc! { "expires_at": { "$gt": now_rfc3339() } }),
                    ]),
                );
            }
            let options = FindOptions::builder()
                .sort(doc! { "occurred_at": -1, "updated_at": -1 })
                .limit(limit)
                .build();
            let mut cursor = db
                .collection::<Document>(COLLECTION)
                .find(filter, options)
                .await
                .map_err(|err| err.to_string())?;
            let mut records = Vec::new();
            while let Some(document) = cursor.try_next().await.map_err(|err| err.to_string())? {
                records.push(from_document(document).map_err(|err| err.to_string())?);
            }
            Ok(records)
        })
    })
    .await
}

pub async fn upsert_pet_activity(
    input: PetActivityInboxUpsert,
) -> Result<PetActivityInboxRecord, String> {
    with_db(|db| {
        Box::pin(async move {
            let now = now_rfc3339();
            let filter = doc! {
                "user_id": &input.user_id,
                "activity_key": &input.activity_key,
                "activity_version": &input.activity_version,
            };
            let mut set = doc! {
                "source": &input.source,
                "kind": &input.kind,
                "title": &input.title,
                "route": to_bson(&input.route).map_err(|err| err.to_string())?,
                "business_status": &input.business_status,
                "requires_action": input.requires_action,
                "occurred_at": &input.occurred_at,
                "updated_at": &now,
            };
            insert_optional(&mut set, "detail", input.detail.as_ref())?;
            insert_optional(&mut set, "event_id", input.event_id.as_ref())?;
            insert_optional(&mut set, "event_sequence", input.event_sequence.as_ref())?;
            insert_optional(&mut set, "metadata", input.metadata.as_ref())?;
            insert_optional(&mut set, "expires_at", input.expires_at.as_ref())?;
            if input.resolved {
                set.insert("inbox_status", PetActivityInboxStatus::Resolved.as_str());
                set.insert("resolved_at", &now);
            }
            let mut set_on_insert = doc! {
                "id": format!("pet_{}", Uuid::new_v4().simple()),
                "user_id": &input.user_id,
                "activity_key": &input.activity_key,
                "activity_version": &input.activity_version,
                "created_at": &now,
            };
            if !input.resolved {
                set_on_insert.insert("inbox_status", PetActivityInboxStatus::Unread.as_str());
            }
            let update = doc! {
                "$set": set,
                "$setOnInsert": set_on_insert,
            };
            let options = FindOneAndUpdateOptions::builder()
                .upsert(true)
                .return_document(ReturnDocument::After)
                .build();
            let document = db
                .collection::<Document>(COLLECTION)
                .find_one_and_update(filter, update, options)
                .await
                .map_err(|err| err.to_string())?
                .ok_or_else(|| "pet activity upsert returned no record".to_string())?;
            from_document(document).map_err(|err| err.to_string())
        })
    })
    .await
}

pub async fn transition_pet_activity(
    user_id: &str,
    activity_id: &str,
    status: PetActivityInboxStatus,
) -> Result<Option<PetActivityInboxRecord>, String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        let activity_id = activity_id.to_string();
        Box::pin(async move {
            let now = now_rfc3339();
            let timestamp_field = match status {
                PetActivityInboxStatus::Displayed => "displayed_at",
                PetActivityInboxStatus::Acknowledged => "acknowledged_at",
                PetActivityInboxStatus::Ignored => "ignored_at",
                PetActivityInboxStatus::Handled => "handled_at",
                PetActivityInboxStatus::Resolved => "resolved_at",
                PetActivityInboxStatus::Expired => "expires_at",
                PetActivityInboxStatus::Unread => "updated_at",
            };
            let options = FindOneAndUpdateOptions::builder()
                .return_document(ReturnDocument::After)
                .build();
            let document = db
                .collection::<Document>(COLLECTION)
                .find_one_and_update(
                    doc! {
                        "id": &activity_id,
                        "user_id": &user_id,
                        "inbox_status": { "$in": ["unread", "displayed"] },
                    },
                    doc! {
                        "$set": {
                            "inbox_status": status.as_str(),
                            timestamp_field: &now,
                            "updated_at": &now,
                        }
                    },
                    options,
                )
                .await
                .map_err(|err| err.to_string())?;
            document
                .map(from_document)
                .transpose()
                .map_err(|err| err.to_string())
        })
    })
    .await
}

pub async fn mark_pet_activities_displayed(
    user_id: &str,
    activity_ids: &[String],
) -> Result<(), String> {
    if activity_ids.is_empty() {
        return Ok(());
    }
    with_db(|db| {
        let user_id = user_id.to_string();
        let activity_ids = activity_ids.to_vec();
        Box::pin(async move {
            let now = now_rfc3339();
            db.collection::<Document>(COLLECTION)
                .update_many(
                    doc! {
                        "user_id": &user_id,
                        "id": { "$in": activity_ids },
                        "inbox_status": "unread",
                    },
                    doc! {
                        "$set": {
                            "inbox_status": "displayed",
                            "displayed_at": &now,
                            "updated_at": &now,
                        }
                    },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            Ok(())
        })
    })
    .await
}

pub async fn update_pet_activity_detail(
    user_id: &str,
    activity_id: &str,
    detail: &str,
) -> Result<(), String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        let activity_id = activity_id.to_string();
        let detail = detail.to_string();
        Box::pin(async move {
            db.collection::<Document>(COLLECTION)
                .update_one(
                    doc! {
                        "user_id": user_id,
                        "id": activity_id,
                        "$or": [
                            { "detail": Bson::Null },
                            { "detail": { "$exists": false } },
                            { "detail": "" },
                        ],
                    },
                    doc! { "$set": { "detail": detail } },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            Ok(())
        })
    })
    .await
}

fn insert_optional<T: serde::Serialize>(
    document: &mut Document,
    key: &str,
    value: Option<&T>,
) -> Result<(), String> {
    document.insert(
        key,
        value
            .map(to_bson)
            .transpose()
            .map_err(|err| err.to_string())?
            .unwrap_or(Bson::Null),
    );
    Ok(())
}
