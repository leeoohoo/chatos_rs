// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};

use crate::core::mongo_cursor::collect_map_sorted_desc;
use crate::core::mongo_query::filter_optional_user_id;
use crate::core::update_fields::mongo_set_doc_from_optional_strings;
use crate::models::terminal::{normalize_terminal_kind, Terminal, TERMINAL_KIND_PROJECT_RUN};
use crate::repositories::db::{
    doc_from_pairs, mongo_delete_one_doc, mongo_find_one_doc, mongo_insert_doc,
    mongo_update_set_doc, to_doc, with_db,
};

fn normalize_doc(doc: &Document) -> Option<Terminal> {
    Some(Terminal {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        cwd: doc.get_str("cwd").ok()?.to_string(),
        kind: normalize_terminal_kind(doc.get_str("kind").ok().map(|s| s.to_string())),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        project_id: doc.get_str("project_id").ok().map(|s| s.to_string()),
        process_id: doc.get_i64("process_id").ok(),
        status: doc.get_str("status").unwrap_or("running").to_string(),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
        last_active_at: doc.get_str("last_active_at").unwrap_or("").to_string(),
    })
}

pub async fn list_terminals_by_kind(
    user_id: Option<String>,
    kind: &str,
) -> Result<Vec<Terminal>, String> {
    with_db(|db| {
        let user_id = user_id.clone();
        let kind = kind.to_string();
        Box::pin(async move {
            let mut filter = filter_optional_user_id(user_id);
            filter.insert("kind", kind);
            let cursor = db
                .collection::<Document>("terminals")
                .find(filter, None)
                .await
                .map_err(|e| e.to_string())?;
            let items: Vec<Terminal> =
                collect_map_sorted_desc(cursor, normalize_doc, |item| item.created_at.as_str())
                    .await?;
            Ok(items)
        })
    })
    .await
}

pub async fn get_terminal_by_id(id: &str) -> Result<Option<Terminal>, String> {
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            let doc = mongo_find_one_doc(db, "terminals", doc! { "id": id }).await?;
            Ok(doc.and_then(|d| normalize_doc(&d)))
        })
    })
    .await
}

pub async fn get_project_run_terminal_by_project_id(
    user_id: Option<String>,
    project_id: &str,
) -> Result<Option<Terminal>, String> {
    let normalized_project_id = project_id.trim().to_string();
    if normalized_project_id.is_empty() {
        return Ok(None);
    }
    with_db(|db| {
        let user_id = user_id.clone();
        let project_id = normalized_project_id.clone();
        Box::pin(async move {
            let mut filter = doc! {
                "project_id": project_id,
                "kind": TERMINAL_KIND_PROJECT_RUN,
            };
            if let Some(uid) = user_id {
                filter.insert("user_id", uid);
            }
            let doc = mongo_find_one_doc(db, "terminals", filter).await?;
            Ok(doc.and_then(|d| normalize_doc(&d)))
        })
    })
    .await
}

pub async fn list_project_run_terminals_by_project_id(
    user_id: Option<String>,
    project_id: &str,
) -> Result<Vec<Terminal>, String> {
    let normalized_project_id = project_id.trim().to_string();
    if normalized_project_id.is_empty() {
        return Ok(Vec::new());
    }
    with_db(|db| {
        let user_id = user_id.clone();
        let project_id = normalized_project_id.clone();
        Box::pin(async move {
            let mut filter = doc! {
                "project_id": project_id,
                "kind": TERMINAL_KIND_PROJECT_RUN,
            };
            if let Some(uid) = user_id {
                filter.insert("user_id", uid);
            }
            let cursor = db
                .collection::<Document>("terminals")
                .find(filter, None)
                .await
                .map_err(|e| e.to_string())?;
            let items: Vec<Terminal> =
                collect_map_sorted_desc(cursor, normalize_doc, |item| item.last_active_at.as_str())
                    .await?;
            Ok(items)
        })
    })
    .await
}

pub async fn create_terminal(terminal: &Terminal) -> Result<String, String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let term_mongo = terminal.clone();

    with_db(|db| {
        let doc = to_doc(doc_from_pairs(vec![
            ("id", Bson::String(term_mongo.id.clone())),
            ("name", Bson::String(term_mongo.name.clone())),
            ("cwd", Bson::String(term_mongo.cwd.clone())),
            ("kind", Bson::String(term_mongo.kind.clone())),
            (
                "user_id",
                crate::core::values::optional_string_bson(term_mongo.user_id.clone()),
            ),
            (
                "project_id",
                crate::core::values::optional_string_bson(term_mongo.project_id.clone()),
            ),
            (
                "process_id",
                term_mongo.process_id.map(Bson::Int64).unwrap_or(Bson::Null),
            ),
            ("status", Bson::String(term_mongo.status.clone())),
            ("created_at", Bson::String(now_mongo.clone())),
            ("updated_at", Bson::String(now_mongo.clone())),
            ("last_active_at", Bson::String(now_mongo.clone())),
        ]));
        Box::pin(async move {
            mongo_insert_doc(db, "terminals", doc).await?;
            Ok(term_mongo.id.clone())
        })
    })
    .await
}

pub async fn update_terminal_status(
    id: &str,
    status: Option<String>,
    last_active_at: Option<String>,
    process_id: Option<i64>,
) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let status_mongo = status.clone();
    let last_mongo = last_active_at.clone().unwrap_or_else(|| now.clone());
    let process_id_mongo = process_id;
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            let mut set_doc = mongo_set_doc_from_optional_strings([("status", status_mongo)]);
            if let Some(pid) = process_id_mongo {
                set_doc.insert("process_id", pid);
            }
            set_doc.insert("updated_at", now_mongo.clone());
            set_doc.insert("last_active_at", last_mongo.clone());
            mongo_update_set_doc(db, "terminals", doc! { "id": id }, set_doc).await?;
            Ok(())
        })
    })
    .await
}

pub async fn touch_terminal(id: &str) -> Result<(), String> {
    update_terminal_status(id, None, Some(crate::core::time::now_rfc3339()), None).await
}

pub async fn delete_terminal(id: &str) -> Result<(), String> {
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            mongo_delete_one_doc(db, "terminals", doc! { "id": &id }).await?;
            Ok(())
        })
    })
    .await
}
