use mongodb::bson::{doc, Bson, Document};
use sqlx::Row;

use crate::core::values::optional_string_bson;
use crate::models::project_run::{ProjectRunCatalog, ProjectRunTarget};
use crate::repositories::db::{to_doc, with_db};

fn parse_targets_from_doc(doc: &Document) -> Vec<ProjectRunTarget> {
    if let Some(Bson::Array(arr)) = doc.get("targets") {
        if let Ok(items) = mongodb::bson::from_bson::<Vec<ProjectRunTarget>>(Bson::Array(arr.clone())) {
            return items;
        }
    }
    if let Ok(raw_json) = doc.get_str("targets_json") {
        if let Ok(items) = serde_json::from_str::<Vec<ProjectRunTarget>>(raw_json) {
            return items;
        }
    }
    Vec::new()
}

fn normalize_doc(doc: &Document) -> Option<ProjectRunCatalog> {
    Some(ProjectRunCatalog {
        project_id: doc.get_str("project_id").ok()?.to_string(),
        user_id: doc.get_str("user_id").ok().map(|v| v.to_string()),
        status: doc.get_str("status").unwrap_or("empty").to_string(),
        default_target_id: doc.get_str("default_target_id").ok().map(|v| v.to_string()),
        targets: parse_targets_from_doc(doc),
        error_message: doc.get_str("error_message").ok().map(|v| v.to_string()),
        analyzed_at: doc.get_str("analyzed_at").ok().map(|v| v.to_string()),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

pub async fn get_catalog_by_project_id(project_id: &str) -> Result<Option<ProjectRunCatalog>, String> {
    with_db(
        |db| {
            let project_id = project_id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("project_run_catalogs")
                    .find_one(doc! { "project_id": project_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.as_ref().and_then(normalize_doc))
            })
        },
        |pool| {
            let project_id = project_id.to_string();
            Box::pin(async move {
                let row = sqlx::query(
                    "SELECT project_id, user_id, status, default_target_id, targets_json, error_message, analyzed_at, updated_at FROM project_run_catalogs WHERE project_id = ?",
                )
                .bind(&project_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                let Some(row) = row else {
                    return Ok(None);
                };
                let targets_json: String = row.try_get("targets_json").unwrap_or_else(|_| "[]".to_string());
                let targets = serde_json::from_str::<Vec<ProjectRunTarget>>(&targets_json).unwrap_or_default();
                Ok(Some(ProjectRunCatalog {
                    project_id: row.try_get::<String, _>("project_id").unwrap_or_default(),
                    user_id: row.try_get::<Option<String>, _>("user_id").unwrap_or(None),
                    status: row
                        .try_get::<String, _>("status")
                        .unwrap_or_else(|_| "empty".to_string()),
                    default_target_id: row.try_get::<Option<String>, _>("default_target_id").unwrap_or(None),
                    targets,
                    error_message: row.try_get::<Option<String>, _>("error_message").unwrap_or(None),
                    analyzed_at: row.try_get::<Option<String>, _>("analyzed_at").unwrap_or(None),
                    updated_at: row.try_get::<String, _>("updated_at").unwrap_or_default(),
                }))
            })
        },
    )
    .await
}

pub async fn upsert_catalog(catalog: &ProjectRunCatalog) -> Result<(), String> {
    let targets_json = serde_json::to_string(&catalog.targets).map_err(|e| e.to_string())?;
    with_db(
        |db| {
            let catalog = catalog.clone();
            let targets_json = targets_json.clone();
            Box::pin(async move {
                let targets_bson = mongodb::bson::to_bson(&catalog.targets).unwrap_or(Bson::Array(vec![]));
                let mut set_doc = Document::new();
                set_doc.insert("project_id", catalog.project_id.clone());
                set_doc.insert("user_id", optional_string_bson(catalog.user_id.clone()));
                set_doc.insert("status", catalog.status.clone());
                set_doc.insert(
                    "default_target_id",
                    optional_string_bson(catalog.default_target_id.clone()),
                );
                set_doc.insert("targets", targets_bson);
                set_doc.insert("targets_json", targets_json);
                set_doc.insert("error_message", optional_string_bson(catalog.error_message.clone()));
                set_doc.insert("analyzed_at", optional_string_bson(catalog.analyzed_at.clone()));
                set_doc.insert("updated_at", catalog.updated_at.clone());
                db.collection::<Document>("project_run_catalogs")
                    .update_one(
                        doc! { "project_id": &catalog.project_id },
                        doc! { "$set": to_doc(set_doc) },
                        mongodb::options::UpdateOptions::builder()
                            .upsert(true)
                            .build(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let catalog = catalog.clone();
            let targets_json = targets_json.clone();
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO project_run_catalogs \
                    (project_id, user_id, status, default_target_id, targets_json, error_message, analyzed_at, updated_at) \
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
                    ON CONFLICT(project_id) DO UPDATE SET \
                    user_id=excluded.user_id, \
                    status=excluded.status, \
                    default_target_id=excluded.default_target_id, \
                    targets_json=excluded.targets_json, \
                    error_message=excluded.error_message, \
                    analyzed_at=excluded.analyzed_at, \
                    updated_at=excluded.updated_at",
                )
                .bind(&catalog.project_id)
                .bind(&catalog.user_id)
                .bind(&catalog.status)
                .bind(&catalog.default_target_id)
                .bind(&targets_json)
                .bind(&catalog.error_message)
                .bind(&catalog.analyzed_at)
                .bind(&catalog.updated_at)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}
