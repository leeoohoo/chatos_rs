use mongodb::bson::{Document, doc};

use crate::db::Db;
use crate::models::EngineThread;

use super::ListThreadsQuery;
use super::common::{collect_threads, normalize_optional_text, thread_collection};

fn push_alias_filter(filters: &mut Vec<Document>, paths: &[&str], value: &str) {
    let aliases = paths
        .iter()
        .map(|path| doc! { *path: value })
        .collect::<Vec<_>>();
    if aliases.len() == 1 {
        if let Some(filter) = aliases.into_iter().next() {
            filters.push(filter);
        }
    } else if !aliases.is_empty() {
        filters.push(doc! { "$or": aliases });
    }
}

pub async fn get_thread_by_id(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<EngineThread>, String> {
    get_thread(db, Some(tenant_id), Some(source_id), thread_id).await
}

pub async fn get_thread(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    thread_id: &str,
) -> Result<Option<EngineThread>, String> {
    let mut filter = doc! {"id": thread_id};
    if let Some(value) = normalize_optional_text(tenant_id) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = normalize_optional_text(source_id) {
        filter.insert("source_id", value);
    }
    thread_collection(db)
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_threads_with_pending_records_by_token_threshold(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    token_threshold: i64,
    limit: i64,
) -> Result<Vec<EngineThread>, String> {
    let effective_threshold = token_threshold.max(1);
    let mut filter = doc! {
        "summary_status": "pending",
        "pending_summary_tokens": { "$gte": effective_threshold },
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }

    let cursor = thread_collection(db)
        .find(filter)
        .sort(doc! {"updated_at": 1, "created_at": 1})
        .limit(limit.max(1).min(500))
        .await
        .map_err(|err| err.to_string())?;
    collect_threads(cursor).await
}

pub async fn list_threads_by_label(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_label: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineThread>, String> {
    let normalized_label = thread_label.trim();
    if normalized_label.is_empty() {
        return Ok(Vec::new());
    }

    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "labels": normalized_label,
    };
    if let Some(value) = status.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("status", value);
    }

    let cursor = thread_collection(db)
        .find(filter)
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(5_000))
        .await
        .map_err(|err| err.to_string())?;

    collect_threads(cursor).await
}

pub async fn list_threads(
    db: &Db,
    query: ListThreadsQuery<'_>,
) -> Result<Vec<EngineThread>, String> {
    let mut filter = Document::new();
    let mut and_filters: Vec<Document> = Vec::new();
    if let Some(value) = normalize_optional_text(query.tenant_id) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = normalize_optional_text(query.source_id) {
        filter.insert("source_id", value);
    }
    if let Some(value) = normalize_optional_text(query.subject_id) {
        filter.insert("subject_id", value);
    }
    if let Some(value) = normalize_optional_text(query.external_thread_id) {
        filter.insert("external_thread_id", value);
    }
    if let Some(value) = normalize_optional_text(query.contact_id) {
        push_alias_filter(
            &mut and_filters,
            &[
                "metadata.legacy_session_mapping.contact_id",
                "metadata.source_metadata.chat_runtime.contact_id",
                "metadata.source_metadata.chat_runtime.contactId",
                "metadata.source_metadata.contact.contact_id",
                "metadata.source_metadata.contact.contactId",
                "metadata.source_metadata.ui_contact.contact_id",
                "metadata.source_metadata.ui_contact.contactId",
            ],
            &value,
        );
    }
    if let Some(value) = normalize_optional_text(query.project_id) {
        push_alias_filter(
            &mut and_filters,
            &[
                "metadata.legacy_session_mapping.project_id",
                "metadata.source_metadata.chat_runtime.project_id",
                "metadata.source_metadata.chat_runtime.projectId",
            ],
            &value,
        );
    }
    if let Some(value) = normalize_optional_text(query.agent_id) {
        push_alias_filter(
            &mut and_filters,
            &[
                "metadata.legacy_session_mapping.agent_id",
                "metadata.source_metadata.chat_runtime.contact_agent_id",
                "metadata.source_metadata.chat_runtime.contactAgentId",
                "metadata.source_metadata.contact.agent_id",
                "metadata.source_metadata.contact.agentId",
                "metadata.source_metadata.ui_contact.agent_id",
                "metadata.source_metadata.ui_contact.agentId",
                "metadata.source_metadata.ui_chat_selection.selected_agent_id",
                "metadata.source_metadata.ui_chat_selection.selectedAgentId",
            ],
            &value,
        );
    }
    if let Some(value) = normalize_optional_text(query.mapping_source) {
        filter.insert("metadata.mapping_source", value);
    }
    if let Some(value) = normalize_optional_text(query.mapping_version) {
        filter.insert("metadata.mapping_version", value);
    }
    if let Some(value) = normalize_optional_text(query.thread_label) {
        filter.insert("labels", value);
    }
    if let Some(value) = normalize_optional_text(query.status) {
        filter.insert("status", value);
    }
    if let Some(value) = normalize_optional_text(query.session_id) {
        push_alias_filter(
            &mut and_filters,
            &["id", "metadata.legacy_session_mapping.session_id"],
            &value,
        );
    }
    if !and_filters.is_empty() {
        filter.insert("$and", and_filters);
    }

    let cursor = thread_collection(db)
        .find(filter)
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .skip(query.offset.max(0) as u64)
        .limit(query.limit.max(1).min(10_000))
        .await
        .map_err(|err| err.to_string())?;

    collect_threads(cursor).await
}
