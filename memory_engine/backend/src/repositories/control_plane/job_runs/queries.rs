// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use mongodb::bson::doc;
use mongodb::options::FindOptions;

use crate::db::Db;
use crate::models::{EngineJobRun, EngineThread};

use super::super::common::job_run_collection;

fn insert_optional_filter(filter: &mut mongodb::bson::Document, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert(key, value);
    }
}

pub async fn list_job_runs(
    db: &Db,
    job_type: Option<&str>,
    trigger_type: Option<&str>,
    thread_id: Option<&str>,
    status: Option<&str>,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<Vec<EngineJobRun>, String> {
    let mut filter = doc! {};
    insert_optional_filter(&mut filter, "job_type", job_type);
    insert_optional_filter(&mut filter, "trigger_type", trigger_type);
    insert_optional_filter(&mut filter, "thread_id", thread_id);
    insert_optional_filter(&mut filter, "status", status);
    insert_optional_filter(&mut filter, "tenant_id", tenant_id);
    insert_optional_filter(&mut filter, "source_id", source_id);

    let options = FindOptions::builder()
        .sort(doc! {"started_at": -1, "id": 1})
        .limit(Some(limit.clamp(1, 1000)))
        .build();
    let cursor = job_run_collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;
    let items: Vec<EngineJobRun> = futures_util::TryStreamExt::try_collect(cursor)
        .await
        .map_err(|err| err.to_string())?;
    enrich_thread_display_names(db, items).await
}

pub async fn get_job_run_by_id(db: &Db, job_run_id: &str) -> Result<Option<EngineJobRun>, String> {
    job_run_collection(db)
        .find_one(doc! {"id": job_run_id})
        .await
        .map_err(|err| err.to_string())
}

pub async fn has_recent_job_run(
    db: &Db,
    job_type: &str,
    trigger_type: Option<&str>,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    within_secs: i64,
) -> Result<bool, String> {
    let normalized_job_type = job_type.trim();
    if normalized_job_type.is_empty() {
        return Ok(false);
    }

    let since = (chrono::Utc::now() - chrono::Duration::seconds(within_secs.max(1))).to_rfc3339();
    let mut filter = doc! {
        "job_type": normalized_job_type,
        "$or": [
            { "status": "running" },
            { "started_at": {"$gte": since} },
        ],
    };
    insert_optional_filter(&mut filter, "trigger_type", trigger_type);
    insert_optional_filter(&mut filter, "tenant_id", tenant_id);
    insert_optional_filter(&mut filter, "source_id", source_id);

    let row = job_run_collection(db)
        .find_one(filter)
        .sort(doc! {"started_at": -1, "id": 1})
        .await
        .map_err(|err| err.to_string())?;
    Ok(row.is_some())
}

async fn enrich_thread_display_names(
    db: &Db,
    mut items: Vec<EngineJobRun>,
) -> Result<Vec<EngineJobRun>, String> {
    let mut thread_filters = Vec::new();
    let mut seen = HashSet::new();

    for run in &items {
        let Some(key) = job_run_thread_lookup_key(
            run.tenant_id.as_deref(),
            run.source_id.as_deref(),
            run.thread_id.as_deref(),
        ) else {
            continue;
        };

        if !seen.insert(key) {
            continue;
        }

        let thread_id = run.thread_id.as_deref().unwrap_or_default().trim();
        let mut filter = doc! { "id": thread_id };
        insert_optional_filter(&mut filter, "tenant_id", run.tenant_id.as_deref());
        insert_optional_filter(&mut filter, "source_id", run.source_id.as_deref());
        thread_filters.push(filter);
    }

    if thread_filters.is_empty() {
        return Ok(items);
    }

    let cursor = db
        .collection::<EngineThread>("engine_threads")
        .find(doc! { "$or": thread_filters })
        .await
        .map_err(|err| err.to_string())?;
    let threads: Vec<EngineThread> = futures_util::TryStreamExt::try_collect(cursor)
        .await
        .map_err(|err| err.to_string())?;

    let thread_names = threads
        .into_iter()
        .map(|thread| {
            (
                format!(
                    "{}::{}::{}",
                    thread.tenant_id.trim(),
                    thread.source_id.trim(),
                    thread.id.trim(),
                ),
                preferred_thread_display_name(&thread),
            )
        })
        .collect::<HashMap<_, _>>();

    for run in &mut items {
        let Some(key) = job_run_thread_lookup_key(
            run.tenant_id.as_deref(),
            run.source_id.as_deref(),
            run.thread_id.as_deref(),
        ) else {
            continue;
        };
        if let Some(display_name) = thread_names.get(&key) {
            run.thread_display_name = Some(display_name.clone());
        }
    }

    Ok(items)
}

fn job_run_thread_lookup_key(
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    thread_id: Option<&str>,
) -> Option<String> {
    let thread_id = thread_id.map(str::trim).filter(|value| !value.is_empty())?;
    Some(format!(
        "{}::{}::{}",
        tenant_id.map(str::trim).unwrap_or_default(),
        source_id.map(str::trim).unwrap_or_default(),
        thread_id,
    ))
}

fn preferred_thread_display_name(thread: &EngineThread) -> String {
    thread
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| thread.subject_id.clone())
}

#[cfg(test)]
mod tests {
    use super::{job_run_thread_lookup_key, preferred_thread_display_name};
    use crate::models::EngineThread;

    #[test]
    fn thread_lookup_key_requires_non_empty_thread_id() {
        assert_eq!(
            job_run_thread_lookup_key(Some("tenant"), Some("source"), None),
            None
        );
        assert_eq!(
            job_run_thread_lookup_key(Some("tenant"), Some("source"), Some("   ")),
            None
        );
    }

    #[test]
    fn thread_lookup_key_trims_and_keeps_scope_parts() {
        let key =
            job_run_thread_lookup_key(Some(" tenant-a "), Some(" source-a "), Some(" thread-a "));

        assert_eq!(key.as_deref(), Some("tenant-a::source-a::thread-a"));
    }

    #[test]
    fn preferred_thread_display_name_prefers_title_then_subject_id() {
        let titled = engine_thread(Some("  Helpful title  "), "subject-a");
        let untitled = engine_thread(Some("   "), "subject-b");

        assert_eq!(preferred_thread_display_name(&titled), "Helpful title");
        assert_eq!(preferred_thread_display_name(&untitled), "subject-b");
    }

    fn engine_thread(title: Option<&str>, subject_id: &str) -> EngineThread {
        EngineThread {
            id: "thread-a".to_string(),
            tenant_id: "tenant-a".to_string(),
            source_id: "source-a".to_string(),
            subject_id: subject_id.to_string(),
            thread_type: "chat".to_string(),
            external_thread_id: None,
            title: title.map(ToOwned::to_owned),
            labels: None,
            metadata: None,
            status: "active".to_string(),
            summary_status: "active".to_string(),
            summary_job_run_id: None,
            summary_locked_at: None,
            summary_lock_expires_at: None,
            pending_record_count: 0,
            pending_summary_tokens: 0,
            created_at: "2026-05-20T00:00:00Z".to_string(),
            updated_at: "2026-05-20T00:00:00Z".to_string(),
            archived_at: None,
        }
    }
}
