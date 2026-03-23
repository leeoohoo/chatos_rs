use std::collections::HashSet;

use chrono::Utc;
use mongodb::bson::{doc, Bson, Document};

use crate::repositories::db::with_db;

pub async fn confirm_change_logs_by_ids(
    change_ids: &[String],
    confirmed_by: Option<&str>,
) -> Result<usize, String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut deduped: Vec<String> = Vec::new();
    for id in change_ids {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            deduped.push(trimmed.to_string());
        }
    }
    if deduped.is_empty() {
        return Ok(0);
    }

    let confirmed_by = confirmed_by
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let now = Utc::now().to_rfc3339();

    with_db(
        |db| {
            let deduped = deduped.clone();
            let confirmed_by = confirmed_by.clone();
            let now = now.clone();
            Box::pin(async move {
                let ids: Vec<Bson> = deduped.into_iter().map(Bson::String).collect();
                let mut set_doc = doc! {
                    "confirmed": true,
                    "confirmed_at": &now,
                };
                if let Some(user_id) = confirmed_by {
                    set_doc.insert("confirmed_by", user_id);
                } else {
                    set_doc.insert("confirmed_by", Bson::Null);
                }
                let filter = doc! {
                    "id": { "$in": ids },
                    "$or": [
                        { "confirmed": { "$exists": false } },
                        { "confirmed": false },
                        { "confirmed": 0 }
                    ]
                };
                let result = db
                    .collection::<Document>("mcp_change_logs")
                    .update_many(filter, doc! { "$set": set_doc }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(result.modified_count as usize)
            })
        },
        |pool| {
            let deduped = deduped.clone();
            let confirmed_by = confirmed_by.clone();
            let now = now.clone();
            Box::pin(async move {
                let placeholders = std::iter::repeat("?")
                    .take(deduped.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "UPDATE mcp_change_logs \
                    SET confirmed = 1, confirmed_at = ?, confirmed_by = ? \
                    WHERE COALESCE(confirmed, 0) = 0 AND id IN ({placeholders})"
                );
                let mut query = sqlx::query(&sql).bind(&now).bind(confirmed_by.as_deref());
                for id in &deduped {
                    query = query.bind(id);
                }
                let result = query.execute(pool).await.map_err(|e| e.to_string())?;
                Ok(result.rows_affected() as usize)
            })
        },
    )
    .await
}
