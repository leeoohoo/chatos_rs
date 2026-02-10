use mongodb::bson::{doc, Bson, Document};
use serde_json::Value;
use sqlx::Row;

use crate::models::user_settings::UserSettings;
use crate::repositories::db::with_db;

pub async fn get_user_settings(user_id: &str) -> Result<Option<UserSettings>, String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("user_settings")
                    .find_one(doc! { "user_id": &user_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(doc) = doc {
                    let settings = doc
                        .get("settings")
                        .cloned()
                        .unwrap_or(Bson::Document(Document::new()));
                    let settings_val = bson_to_json(settings);
                    return Ok(Some(UserSettings {
                        user_id,
                        settings: settings_val,
                    }));
                }
                Ok(None)
            })
        },
        |pool| {
            let user_id = user_id.to_string();
            Box::pin(async move {
                let row = sqlx::query(
                    "SELECT user_id, settings, updated_at FROM user_settings WHERE user_id = ?",
                )
                .bind(&user_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                if let Some(row) = row {
                    let settings_str: Option<String> = row.try_get("settings").ok();
                    let settings_val = settings_str
                        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                        .unwrap_or(Value::Object(serde_json::Map::new()));
                    return Ok(Some(UserSettings {
                        user_id,
                        settings: settings_val,
                    }));
                }
                Ok(None)
            })
        },
    )
    .await
}

pub async fn set_user_settings(user_id: &str, settings: &Value) -> Result<(), String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            let settings = settings.clone();
            Box::pin(async move {
                let now = chrono::Utc::now().to_rfc3339();
                db.collection::<Document>("user_settings")
                    .update_one(doc! { "user_id": &user_id }, doc! { "$set": { "user_id": &user_id, "settings": json_to_bson(settings), "updated_at": &now } }, mongodb::options::UpdateOptions::builder().upsert(true).build())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let user_id = user_id.to_string();
            let settings = settings.clone();
            Box::pin(async move {
                let json = settings.to_string();
                sqlx::query("INSERT INTO user_settings (user_id, settings, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP) ON CONFLICT(user_id) DO UPDATE SET settings = excluded.settings, updated_at = CURRENT_TIMESTAMP")
                    .bind(&user_id)
                    .bind(&json)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        }
    ).await
}

pub async fn update_user_settings(user_id: &str, patch: &Value) -> Result<Value, String> {
    let existing = get_user_settings(user_id).await?;
    let mut merged = match existing {
        Some(row) => row.settings,
        None => Value::Object(serde_json::Map::new()),
    };
    if let Value::Object(map) = patch {
        if let Value::Object(ref mut target) = merged {
            for (k, v) in map {
                target.insert(k.clone(), v.clone());
            }
        }
    }
    set_user_settings(user_id, &merged).await?;
    Ok(merged)
}

fn bson_to_json(bson: Bson) -> Value {
    serde_json::to_value(bson).unwrap_or(Value::Null)
}

fn json_to_bson(json: Value) -> Bson {
    mongodb::bson::to_bson(&json).unwrap_or(Bson::Null)
}
