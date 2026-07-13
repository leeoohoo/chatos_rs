// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};
use mongodb::options::UpdateOptions;

use crate::core::values::optional_string_bson;
use crate::models::project_run::{ProjectRunCatalog, ProjectRunTarget};
use crate::repositories::db::{mongo_find_one_doc, with_db};

fn parse_targets_from_doc(doc: &Document) -> Vec<ProjectRunTarget> {
    if let Some(Bson::Array(arr)) = doc.get("targets") {
        if let Ok(items) =
            mongodb::bson::from_bson::<Vec<ProjectRunTarget>>(Bson::Array(arr.clone()))
        {
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

pub async fn get_catalog_by_project_id(
    project_id: &str,
) -> Result<Option<ProjectRunCatalog>, String> {
    with_db(|db| {
        let project_id = project_id.to_string();
        Box::pin(async move {
            let doc = mongo_find_one_doc(
                db,
                "project_run_catalogs",
                doc! { "project_id": project_id },
            )
            .await?;
            Ok(doc.as_ref().and_then(normalize_doc))
        })
    })
    .await
}

pub async fn upsert_catalog(catalog: &ProjectRunCatalog) -> Result<(), String> {
    let targets_json = serde_json::to_string(&catalog.targets).map_err(|e| e.to_string())?;
    with_db(|db| {
        let catalog = catalog.clone();
        let targets_json = targets_json.clone();
        Box::pin(async move {
            let targets_bson =
                mongodb::bson::to_bson(&catalog.targets).unwrap_or(Bson::Array(vec![]));
            let mut set_doc = Document::new();
            set_doc.insert("project_id", catalog.project_id.clone());
            set_doc.insert("status", catalog.status.clone());
            set_doc.insert("targets", targets_bson);
            set_doc.insert("targets_json", targets_json);
            set_doc.insert("updated_at", catalog.updated_at.clone());
            let mut unset_doc = Document::new();
            set_or_unset_optional_string(
                &mut set_doc,
                &mut unset_doc,
                "user_id",
                catalog.user_id.clone(),
            );
            set_or_unset_optional_string(
                &mut set_doc,
                &mut unset_doc,
                "default_target_id",
                catalog.default_target_id.clone(),
            );
            set_or_unset_optional_string(
                &mut set_doc,
                &mut unset_doc,
                "error_message",
                catalog.error_message.clone(),
            );
            set_or_unset_optional_string(
                &mut set_doc,
                &mut unset_doc,
                "analyzed_at",
                catalog.analyzed_at.clone(),
            );
            let mut update_doc = doc! { "$set": set_doc };
            if !unset_doc.is_empty() {
                update_doc.insert("$unset", unset_doc);
            }
            db.collection::<Document>("project_run_catalogs")
                .update_one(
                    doc! { "project_id": &catalog.project_id },
                    update_doc,
                    Some(UpdateOptions::builder().upsert(true).build()),
                )
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    })
    .await
}

fn set_or_unset_optional_string(
    set_doc: &mut Document,
    unset_doc: &mut Document,
    field: &str,
    value: Option<String>,
) {
    match optional_string_bson(value) {
        Bson::String(value) => {
            set_doc.insert(field, value);
        }
        _ => {
            unset_doc.insert(field, "");
        }
    }
}
