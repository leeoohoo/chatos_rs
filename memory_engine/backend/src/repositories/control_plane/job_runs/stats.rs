use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};

use crate::db::Db;

use super::super::common::doc_i64;

fn insert_optional_match(match_doc: &mut mongodb::bson::Document, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        match_doc.insert(key, value);
    }
}

pub async fn job_run_stats(
    db: &Db,
    job_type: Option<&str>,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    since_hours: i64,
) -> Result<serde_json::Value, String> {
    let mut match_doc = doc! {
        "started_at": {
            "$gte": (chrono::Utc::now() - chrono::Duration::hours(since_hours.max(1))).to_rfc3339()
        }
    };
    insert_optional_match(&mut match_doc, "job_type", job_type);
    insert_optional_match(&mut match_doc, "tenant_id", tenant_id);
    insert_optional_match(&mut match_doc, "source_id", source_id);

    let pipeline = vec![
        doc! {"$match": match_doc},
        doc! {"$group": {"_id": {"job_type": "$job_type", "status": "$status"}, "count": {"$sum": 1}}},
    ];

    let cursor = db
        .collection::<Document>("engine_job_runs")
        .aggregate(pipeline)
        .await
        .map_err(|err| err.to_string())?;
    let docs: Vec<Document> = cursor.try_collect().await.map_err(|err| err.to_string())?;

    let mut map = serde_json::Map::new();
    for doc in docs {
        let Some(id_doc) = doc.get_document("_id").ok() else {
            continue;
        };
        let Some(job_type) = id_doc.get_str("job_type").ok() else {
            continue;
        };
        let Some(status) = id_doc.get_str("status").ok() else {
            continue;
        };
        let count = doc_i64(&doc, "count");

        let entry = map
            .entry(job_type.to_string())
            .or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = entry.as_object_mut() {
            obj.insert(status.to_string(), serde_json::json!(count));
        }
    }

    Ok(serde_json::Value::Object(map))
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{Bson, Document, doc};

    use crate::repositories::control_plane::common::doc_i64;

    #[test]
    fn doc_i64_handles_supported_number_types() {
        assert_eq!(doc_i64(&doc! { "count": 3i32 }, "count"), 3);
        assert_eq!(doc_i64(&doc! { "count": 4i64 }, "count"), 4);
        assert_eq!(doc_i64(&doc! { "count": 5.0f64 }, "count"), 5);
    }

    #[test]
    fn doc_i64_defaults_to_zero_for_missing_or_invalid_values() {
        assert_eq!(doc_i64(&Document::new(), "count"), 0);
        assert_eq!(
            doc_i64(&doc! { "count": Bson::String("oops".to_string()) }, "count"),
            0
        );
    }
}
