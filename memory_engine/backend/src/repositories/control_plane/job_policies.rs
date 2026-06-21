use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use crate::db::Db;
use crate::models::{
    EngineJobPolicy, PROMPT_LANGUAGE_EN, PROMPT_LANGUAGE_ZH, UpsertEngineJobPolicyRequest,
    now_rfc3339,
};

use super::common::{
    default_job_policy, default_job_types, job_policy_collection, normalize_job_policy,
};

pub async fn list_job_policies(db: &Db) -> Result<Vec<EngineJobPolicy>, String> {
    let options = FindOptions::builder().sort(doc! {"job_type": 1}).build();
    let cursor = job_policy_collection(db)
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;
    let mut items: Vec<EngineJobPolicy> =
        cursor.try_collect().await.map_err(|err| err.to_string())?;

    for item in &mut items {
        normalize_job_policy(item);
    }

    for job_type in default_job_types() {
        if !items.iter().any(|item| item.job_type == *job_type) {
            items.push(default_job_policy(job_type));
        }
    }
    items.sort_by(|a, b| a.job_type.cmp(&b.job_type));
    Ok(items)
}

pub async fn count_job_policies(db: &Db) -> Result<i64, String> {
    let stored_count = job_policy_collection(db)
        .count_documents(doc! {})
        .await
        .map(|count| count as i64)
        .map_err(|err| err.to_string())?;
    Ok(stored_count.max(default_job_types().len() as i64))
}

pub async fn get_job_policy(db: &Db, job_type: &str) -> Result<Option<EngineJobPolicy>, String> {
    job_policy_collection(db)
        .find_one(doc! {"job_type": job_type})
        .await
        .map_err(|err| err.to_string())
}

pub async fn get_effective_job_policy(db: &Db, job_type: &str) -> Result<EngineJobPolicy, String> {
    if let Some(mut policy) = get_job_policy(db, job_type).await? {
        normalize_job_policy(&mut policy);
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
        let normalized = normalize_prompt_update(value);
        current.summary_prompt = normalized.clone();
        current.summary_prompt_zh = normalized;
    }
    if let Some(value) = req.summary_prompt_zh {
        current.summary_prompt_zh = normalize_prompt_update(value);
    }
    if let Some(value) = req.summary_prompt_en {
        current.summary_prompt_en = normalize_prompt_update(value);
    }
    if let Some(value) = req.summary_prompt_language {
        current.summary_prompt_language = normalize_prompt_language(value.as_str()).to_string();
    }
    if let Some(value) = req.rollup_summary_prompt {
        let normalized = normalize_prompt_update(value);
        current.rollup_summary_prompt = normalized.clone();
        current.rollup_summary_prompt_zh = normalized;
    }
    if let Some(value) = req.rollup_summary_prompt_zh {
        current.rollup_summary_prompt_zh = normalize_prompt_update(value);
    }
    if let Some(value) = req.rollup_summary_prompt_en {
        current.rollup_summary_prompt_en = normalize_prompt_update(value);
    }
    if let Some(value) = req.rollup_summary_prompt_language {
        current.rollup_summary_prompt_language =
            normalize_prompt_language(value.as_str()).to_string();
    }
    if let Some(value) = req.token_limit {
        current.token_limit = value.map(|v| v.max(128));
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
    if let Some(value) = req.count_limit {
        current.count_limit = value.map(|v| v.max(1));
    }
    if let Some(value) = req.keep_level0_count {
        current.keep_level0_count = value.map(|v| v.max(0));
    }
    if let Some(value) = req.max_level {
        current.max_level = value.map(|v| v.max(1));
    }
    normalize_job_policy(&mut current);
    current.updated_at = now_rfc3339();

    job_policy_collection(db)
        .replace_one(doc! {"job_type": &current.job_type}, current.clone())
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    Ok(current)
}

fn normalize_prompt_update(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn normalize_prompt_language(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case(PROMPT_LANGUAGE_EN) {
        PROMPT_LANGUAGE_EN
    } else {
        PROMPT_LANGUAGE_ZH
    }
}
