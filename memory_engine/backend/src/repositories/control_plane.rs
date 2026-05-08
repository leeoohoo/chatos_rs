use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    now_rfc3339, CreateEngineJobRunRequest, EngineJobPolicy, EngineJobRun, EngineModelProfile,
    FinishEngineJobRunRequest, UpsertEngineJobPolicyRequest, UpsertEngineModelProfileRequest,
    DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE,
};

const JOB_TYPE_SUMMARY: &str = "summary";
const JOB_TYPE_ROLLUP: &str = "rollup";
const JOB_TYPE_SUBJECT_MEMORY: &str = "subject_memory";
const JOB_TYPE_THREAD_REPAIR: &str = "thread_repair";

fn model_profile_collection(db: &Db) -> mongodb::Collection<EngineModelProfile> {
    db.collection::<EngineModelProfile>("engine_model_profiles")
}

fn job_policy_collection(db: &Db) -> mongodb::Collection<EngineJobPolicy> {
    db.collection::<EngineJobPolicy>("engine_job_policies")
}

fn job_run_collection(db: &Db) -> mongodb::Collection<EngineJobRun> {
    db.collection::<EngineJobRun>("engine_job_runs")
}

fn doc_i64(doc: &Document, key: &str) -> i64 {
    match doc.get(key) {
        Some(Bson::Int32(v)) => *v as i64,
        Some(Bson::Int64(v)) => *v,
        Some(Bson::Double(v)) => *v as i64,
        _ => 0,
    }
}

pub fn default_job_types() -> &'static [&'static str] {
    &[
        JOB_TYPE_SUMMARY,
        JOB_TYPE_ROLLUP,
        JOB_TYPE_SUBJECT_MEMORY,
        JOB_TYPE_THREAD_REPAIR,
    ]
}

pub fn default_summary_job_policy() -> EngineJobPolicy {
    EngineJobPolicy {
        job_type: JOB_TYPE_SUMMARY.to_string(),
        enabled: true,
        model_profile_id: None,
        summary_prompt: Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string()),
        token_limit: Some(6000),
        round_limit: Some(8),
        target_summary_tokens: Some(700),
        interval_seconds: Some(30),
        max_threads_per_tick: Some(50),
        keep_level0_count: None,
        max_level: None,
        max_records_per_thread: Some(50),
        updated_at: now_rfc3339(),
    }
}

pub fn default_rollup_job_policy() -> EngineJobPolicy {
    EngineJobPolicy {
        job_type: JOB_TYPE_ROLLUP.to_string(),
        enabled: true,
        model_profile_id: None,
        summary_prompt: Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string()),
        token_limit: Some(6000),
        round_limit: Some(8),
        target_summary_tokens: Some(700),
        interval_seconds: Some(60),
        max_threads_per_tick: Some(50),
        keep_level0_count: Some(5),
        max_level: Some(4),
        max_records_per_thread: None,
        updated_at: now_rfc3339(),
    }
}

pub fn default_subject_memory_job_policy() -> EngineJobPolicy {
    EngineJobPolicy {
        job_type: JOB_TYPE_SUBJECT_MEMORY.to_string(),
        enabled: true,
        model_profile_id: None,
        summary_prompt: Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string()),
        token_limit: Some(6000),
        round_limit: Some(20),
        target_summary_tokens: Some(700),
        interval_seconds: Some(60),
        max_threads_per_tick: Some(50),
        keep_level0_count: Some(5),
        max_level: Some(4),
        max_records_per_thread: None,
        updated_at: now_rfc3339(),
    }
}

pub fn default_thread_repair_job_policy() -> EngineJobPolicy {
    EngineJobPolicy {
        job_type: JOB_TYPE_THREAD_REPAIR.to_string(),
        enabled: true,
        model_profile_id: None,
        summary_prompt: Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string()),
        token_limit: Some(6000),
        round_limit: Some(8),
        target_summary_tokens: Some(700),
        interval_seconds: Some(60),
        max_threads_per_tick: Some(5000),
        keep_level0_count: None,
        max_level: None,
        max_records_per_thread: Some(50),
        updated_at: now_rfc3339(),
    }
}

fn default_job_policy(job_type: &str) -> EngineJobPolicy {
    match job_type.trim() {
        JOB_TYPE_SUMMARY => default_summary_job_policy(),
        JOB_TYPE_ROLLUP => default_rollup_job_policy(),
        JOB_TYPE_SUBJECT_MEMORY => default_subject_memory_job_policy(),
        JOB_TYPE_THREAD_REPAIR => default_thread_repair_job_policy(),
        other => EngineJobPolicy {
            job_type: other.to_string(),
            enabled: true,
            model_profile_id: None,
            summary_prompt: Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string()),
            token_limit: None,
            round_limit: None,
            target_summary_tokens: None,
            interval_seconds: None,
            max_threads_per_tick: None,
            keep_level0_count: None,
            max_level: None,
            max_records_per_thread: None,
            updated_at: now_rfc3339(),
        },
    }
}

pub async fn list_model_profiles(db: &Db) -> Result<Vec<EngineModelProfile>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"enabled": -1, "updated_at": -1})
        .build();
    let cursor = model_profile_collection(db)
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;
    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub async fn get_model_profile_by_id(
    db: &Db,
    id: &str,
) -> Result<Option<EngineModelProfile>, String> {
    model_profile_collection(db)
        .find_one(doc! {"id": id})
        .await
        .map_err(|err| err.to_string())
}

pub async fn get_active_model_profile(db: &Db) -> Result<Option<EngineModelProfile>, String> {
    model_profile_collection(db)
        .find_one(doc! {"enabled": true})
        .sort(doc! {"updated_at": -1})
        .await
        .map_err(|err| err.to_string())
}

pub async fn create_model_profile(
    db: &Db,
    req: UpsertEngineModelProfileRequest,
) -> Result<EngineModelProfile, String> {
    let now = now_rfc3339();
    let profile = EngineModelProfile {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        provider: req.provider,
        model: req.model,
        base_url: req.base_url,
        api_key: req.api_key,
        supports_images: req.supports_images.unwrap_or(false),
        supports_reasoning: req.supports_reasoning.unwrap_or(false),
        supports_responses: req.supports_responses.unwrap_or(false),
        temperature: req.temperature,
        thinking_level: req.thinking_level,
        enabled: req.enabled.unwrap_or(true),
        created_at: now.clone(),
        updated_at: now,
    };

    model_profile_collection(db)
        .insert_one(profile.clone())
        .await
        .map_err(|err| err.to_string())?;
    Ok(profile)
}

pub async fn update_model_profile(
    db: &Db,
    id: &str,
    req: UpsertEngineModelProfileRequest,
) -> Result<Option<EngineModelProfile>, String> {
    let Some(existing) = get_model_profile_by_id(db, id).await? else {
        return Ok(None);
    };

    let updated = EngineModelProfile {
        id: existing.id,
        name: req.name,
        provider: req.provider,
        model: req.model,
        base_url: req.base_url.or(existing.base_url),
        api_key: req.api_key.or(existing.api_key),
        supports_images: req.supports_images.unwrap_or(existing.supports_images),
        supports_reasoning: req
            .supports_reasoning
            .unwrap_or(existing.supports_reasoning),
        supports_responses: req
            .supports_responses
            .unwrap_or(existing.supports_responses),
        temperature: req.temperature.or(existing.temperature),
        thinking_level: req.thinking_level.or(existing.thinking_level),
        enabled: req.enabled.unwrap_or(existing.enabled),
        created_at: existing.created_at,
        updated_at: now_rfc3339(),
    };

    model_profile_collection(db)
        .replace_one(doc! {"id": id}, updated.clone())
        .await
        .map_err(|err| err.to_string())?;
    Ok(Some(updated))
}

pub async fn delete_model_profile(db: &Db, id: &str) -> Result<bool, String> {
    let result = model_profile_collection(db)
        .delete_one(doc! {"id": id})
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.deleted_count > 0)
}

pub async fn list_job_policies(db: &Db) -> Result<Vec<EngineJobPolicy>, String> {
    let options = FindOptions::builder().sort(doc! {"job_type": 1}).build();
    let cursor = job_policy_collection(db)
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;
    let mut items: Vec<EngineJobPolicy> =
        cursor.try_collect().await.map_err(|err| err.to_string())?;

    for job_type in default_job_types() {
        if !items.iter().any(|item| item.job_type == *job_type) {
            items.push(default_job_policy(job_type));
        }
    }
    items.sort_by(|a, b| a.job_type.cmp(&b.job_type));
    Ok(items)
}

pub async fn get_job_policy(
    db: &Db,
    job_type: &str,
) -> Result<Option<EngineJobPolicy>, String> {
    job_policy_collection(db)
        .find_one(doc! {"job_type": job_type})
        .await
        .map_err(|err| err.to_string())
}

pub async fn get_effective_job_policy(
    db: &Db,
    job_type: &str,
) -> Result<EngineJobPolicy, String> {
    if let Some(policy) = get_job_policy(db, job_type).await? {
        return Ok(policy);
    }
    Ok(default_job_policy(job_type))
}

pub async fn upsert_job_policy(
    db: &Db,
    job_type: &str,
    req: UpsertEngineJobPolicyRequest,
) -> Result<EngineJobPolicy, String> {
    let mut current = get_job_policy(db, job_type)
        .await?
        .unwrap_or_else(|| default_job_policy(job_type));

    if let Some(value) = req.enabled {
        current.enabled = value;
    }
    if let Some(value) = req.model_profile_id {
        current.model_profile_id = value.filter(|v| !v.trim().is_empty());
    }
    if let Some(value) = req.summary_prompt {
        current.summary_prompt = value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    }
    if let Some(value) = req.token_limit {
        current.token_limit = value.map(|v| v.max(128));
    }
    if let Some(value) = req.round_limit {
        current.round_limit = value.map(|v| v.max(1));
    }
    if let Some(value) = req.target_summary_tokens {
        current.target_summary_tokens = value.map(|v| v.max(128));
    }
    if let Some(value) = req.interval_seconds {
        current.interval_seconds = value.map(|v| v.max(3));
    }
    if let Some(value) = req.max_threads_per_tick {
        current.max_threads_per_tick = value.map(|v| v.max(1));
    }
    if let Some(value) = req.keep_level0_count {
        current.keep_level0_count = value.map(|v| v.max(0));
    }
    if let Some(value) = req.max_level {
        current.max_level = value.map(|v| v.max(1));
    }
    if let Some(value) = req.max_records_per_thread {
        current.max_records_per_thread = value.map(|v| v.max(1));
    }
    if current.summary_prompt.is_none() {
        current.summary_prompt = Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string());
    }
    current.updated_at = now_rfc3339();

    job_policy_collection(db)
        .replace_one(doc! {"job_type": &current.job_type}, current.clone())
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    Ok(current)
}

pub async fn create_job_run(
    db: &Db,
    req: CreateEngineJobRunRequest,
) -> Result<EngineJobRun, String> {
    let started_at = now_rfc3339();
    let job_run = EngineJobRun {
        id: Uuid::new_v4().to_string(),
        job_type: req.job_type,
        trigger_type: req.trigger_type,
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        thread_id: req.thread_id,
        subject_id: req.subject_id,
        thread_label: req.thread_label,
        status: "running".to_string(),
        input_count: 0,
        output_count: 0,
        processed_count: 0,
        success_count: 0,
        error_count: 0,
        metadata: req.metadata,
        error_message: None,
        started_at,
        finished_at: None,
    };

    job_run_collection(db)
        .insert_one(job_run.clone())
        .await
        .map_err(|err| err.to_string())?;
    Ok(job_run)
}

pub async fn finish_job_run(
    db: &Db,
    id: &str,
    req: FinishEngineJobRunRequest,
) -> Result<Option<EngineJobRun>, String> {
    let finished_at = now_rfc3339();

    job_run_collection(db)
        .update_one(
            doc! {"id": id},
            doc! {
                "$set": {
                    "status": req.status,
                    "input_count": req.input_count,
                    "output_count": req.output_count,
                    "processed_count": req.processed_count,
                    "success_count": req.success_count,
                    "error_count": req.error_count,
                    "metadata": mongodb::bson::to_bson(&req.metadata).unwrap_or(mongodb::bson::Bson::Null),
                    "error_message": mongodb::bson::to_bson(&req.error_message).unwrap_or(mongodb::bson::Bson::Null),
                    "finished_at": finished_at,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    job_run_collection(db)
        .find_one(doc! {"id": id})
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_job_runs(
    db: &Db,
    job_type: Option<&str>,
    thread_id: Option<&str>,
    status: Option<&str>,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<Vec<EngineJobRun>, String> {
    let mut filter = doc! {};
    if let Some(value) = job_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("job_type", value);
    }
    if let Some(value) = thread_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("thread_id", value);
    }
    if let Some(value) = status.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("status", value);
    }
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }

    let options = FindOptions::builder()
        .sort(doc! {"started_at": -1})
        .limit(Some(limit.max(1).min(1000)))
        .build();
    let cursor = job_run_collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;
    cursor.try_collect().await.map_err(|err| err.to_string())
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
    if let Some(value) = job_type.map(str::trim).filter(|value| !value.is_empty()) {
        match_doc.insert("job_type", value);
    }
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        match_doc.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        match_doc.insert("source_id", value);
    }

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
