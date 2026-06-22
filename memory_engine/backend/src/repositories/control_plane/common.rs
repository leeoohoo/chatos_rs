use mongodb::bson::{Bson, Document};

use crate::db::Db;
use crate::models::{
    now_rfc3339, EngineJobPolicy, EngineJobRun, EngineModelProfile,
    DEFAULT_ENGINE_MEMORY_ROLLUP_PROMPT_TEMPLATE, DEFAULT_ENGINE_MEMORY_ROLLUP_PROMPT_TEMPLATE_EN,
    DEFAULT_ENGINE_ROLLUP_PROMPT_TEMPLATE, DEFAULT_ENGINE_ROLLUP_PROMPT_TEMPLATE_EN,
    DEFAULT_ENGINE_SUBJECT_MEMORY_PROMPT_TEMPLATE,
    DEFAULT_ENGINE_SUBJECT_MEMORY_PROMPT_TEMPLATE_EN, DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE,
    DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE_EN, DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE,
    DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE_EN, PROMPT_LANGUAGE_EN, PROMPT_LANGUAGE_ZH,
};

pub(crate) const STALE_THREAD_REPAIR_JOB_TIMEOUT_SECS: i64 = 1800;
pub(crate) const DEFAULT_THREAD_REPAIR_TOKEN_LIMIT: i64 = 200000;

pub(crate) const JOB_TYPE_SUMMARY: &str = "summary";
pub(crate) const JOB_TYPE_ROLLUP: &str = "rollup";
pub(crate) const JOB_TYPE_SUBJECT_MEMORY: &str = "subject_memory";
pub(crate) const JOB_TYPE_THREAD_REPAIR: &str = "thread_repair";

pub(crate) fn model_profile_collection(db: &Db) -> mongodb::Collection<EngineModelProfile> {
    db.collection::<EngineModelProfile>("engine_model_profiles")
}

pub(crate) fn job_policy_collection(db: &Db) -> mongodb::Collection<EngineJobPolicy> {
    db.collection::<EngineJobPolicy>("engine_job_policies")
}

pub(crate) fn job_run_collection(db: &Db) -> mongodb::Collection<EngineJobRun> {
    db.collection::<EngineJobRun>("engine_job_runs")
}

pub(crate) fn doc_i64(doc: &Document, key: &str) -> i64 {
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
        summary_prompt_zh: Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string()),
        summary_prompt_en: Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE_EN.to_string()),
        summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
        rollup_summary_prompt: None,
        rollup_summary_prompt_zh: None,
        rollup_summary_prompt_en: None,
        rollup_summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
        token_limit: Some(6000),
        target_summary_tokens: Some(700),
        interval_seconds: Some(60),
        max_threads_per_tick: Some(10),
        count_limit: None,
        keep_level0_count: None,
        max_level: None,
        updated_at: now_rfc3339(),
    }
}

pub fn default_rollup_job_policy() -> EngineJobPolicy {
    EngineJobPolicy {
        job_type: JOB_TYPE_ROLLUP.to_string(),
        enabled: true,
        model_profile_id: None,
        summary_prompt: Some(DEFAULT_ENGINE_ROLLUP_PROMPT_TEMPLATE.to_string()),
        summary_prompt_zh: Some(DEFAULT_ENGINE_ROLLUP_PROMPT_TEMPLATE.to_string()),
        summary_prompt_en: Some(DEFAULT_ENGINE_ROLLUP_PROMPT_TEMPLATE_EN.to_string()),
        summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
        rollup_summary_prompt: None,
        rollup_summary_prompt_zh: None,
        rollup_summary_prompt_en: None,
        rollup_summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
        token_limit: Some(6000),
        target_summary_tokens: Some(700),
        interval_seconds: Some(120),
        max_threads_per_tick: Some(8),
        count_limit: None,
        keep_level0_count: Some(5),
        max_level: Some(4),
        updated_at: now_rfc3339(),
    }
}

pub fn default_subject_memory_job_policy() -> EngineJobPolicy {
    EngineJobPolicy {
        job_type: JOB_TYPE_SUBJECT_MEMORY.to_string(),
        enabled: true,
        model_profile_id: None,
        summary_prompt: Some(DEFAULT_ENGINE_SUBJECT_MEMORY_PROMPT_TEMPLATE.to_string()),
        summary_prompt_zh: Some(DEFAULT_ENGINE_SUBJECT_MEMORY_PROMPT_TEMPLATE.to_string()),
        summary_prompt_en: Some(DEFAULT_ENGINE_SUBJECT_MEMORY_PROMPT_TEMPLATE_EN.to_string()),
        summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
        rollup_summary_prompt: Some(DEFAULT_ENGINE_MEMORY_ROLLUP_PROMPT_TEMPLATE.to_string()),
        rollup_summary_prompt_zh: Some(DEFAULT_ENGINE_MEMORY_ROLLUP_PROMPT_TEMPLATE.to_string()),
        rollup_summary_prompt_en: Some(DEFAULT_ENGINE_MEMORY_ROLLUP_PROMPT_TEMPLATE_EN.to_string()),
        rollup_summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
        token_limit: Some(6000),
        target_summary_tokens: Some(700),
        interval_seconds: Some(180),
        max_threads_per_tick: Some(5),
        count_limit: None,
        keep_level0_count: Some(5),
        max_level: Some(4),
        updated_at: now_rfc3339(),
    }
}

pub fn default_thread_repair_job_policy() -> EngineJobPolicy {
    EngineJobPolicy {
        job_type: JOB_TYPE_THREAD_REPAIR.to_string(),
        enabled: true,
        model_profile_id: None,
        summary_prompt: Some(DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE.to_string()),
        summary_prompt_zh: Some(DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE.to_string()),
        summary_prompt_en: Some(DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE_EN.to_string()),
        summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
        rollup_summary_prompt: None,
        rollup_summary_prompt_zh: None,
        rollup_summary_prompt_en: None,
        rollup_summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
        token_limit: Some(DEFAULT_THREAD_REPAIR_TOKEN_LIMIT),
        target_summary_tokens: None,
        interval_seconds: Some(60),
        max_threads_per_tick: None,
        count_limit: None,
        keep_level0_count: None,
        max_level: None,
        updated_at: now_rfc3339(),
    }
}

pub(crate) fn default_job_policy(job_type: &str) -> EngineJobPolicy {
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
            summary_prompt_zh: Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string()),
            summary_prompt_en: Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE_EN.to_string()),
            summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
            rollup_summary_prompt: None,
            rollup_summary_prompt_zh: None,
            rollup_summary_prompt_en: None,
            rollup_summary_prompt_language: PROMPT_LANGUAGE_ZH.to_string(),
            token_limit: None,
            target_summary_tokens: None,
            interval_seconds: None,
            max_threads_per_tick: None,
            count_limit: None,
            keep_level0_count: None,
            max_level: None,
            updated_at: now_rfc3339(),
        },
    }
}

pub(crate) fn normalize_job_policy(policy: &mut EngineJobPolicy) {
    if policy.job_type == JOB_TYPE_THREAD_REPAIR {
        drop_legacy_summary_prompt_for_thread_repair(policy);
    }
    normalize_policy_prompts(policy);
    if policy.job_type == JOB_TYPE_THREAD_REPAIR {
        policy.token_limit = Some(
            policy
                .token_limit
                .unwrap_or(DEFAULT_THREAD_REPAIR_TOKEN_LIMIT)
                .max(128),
        );
        policy.target_summary_tokens = None;
        policy.max_threads_per_tick = None;
    }
}

fn drop_legacy_summary_prompt_for_thread_repair(policy: &mut EngineJobPolicy) {
    if prompt_matches_default_summary(policy.summary_prompt.as_deref()) {
        policy.summary_prompt = None;
    }
    if prompt_matches_default_summary(policy.summary_prompt_zh.as_deref()) {
        policy.summary_prompt_zh = None;
    }
    if prompt_matches_default_summary(policy.summary_prompt_en.as_deref()) {
        policy.summary_prompt_en = None;
    }
}

fn prompt_matches_default_summary(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|text| {
        text == DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.trim()
            || text == DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE_EN.trim()
    })
}

fn normalize_policy_prompts(policy: &mut EngineJobPolicy) {
    let default_policy = default_job_policy(policy.job_type.as_str());
    let legacy_summary_prompt = DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.trim();

    normalize_prompt_group(
        &mut policy.summary_prompt,
        &mut policy.summary_prompt_zh,
        &mut policy.summary_prompt_en,
        &mut policy.summary_prompt_language,
        default_policy.summary_prompt.as_deref(),
        default_policy.summary_prompt_zh.as_deref(),
        default_policy.summary_prompt_en.as_deref(),
        legacy_summary_prompt,
    );

    normalize_prompt_group(
        &mut policy.rollup_summary_prompt,
        &mut policy.rollup_summary_prompt_zh,
        &mut policy.rollup_summary_prompt_en,
        &mut policy.rollup_summary_prompt_language,
        default_policy.rollup_summary_prompt.as_deref(),
        default_policy.rollup_summary_prompt_zh.as_deref(),
        default_policy.rollup_summary_prompt_en.as_deref(),
        legacy_summary_prompt,
    );
}

fn normalize_prompt_group(
    active_prompt: &mut Option<String>,
    prompt_zh: &mut Option<String>,
    prompt_en: &mut Option<String>,
    prompt_language: &mut String,
    default_active_prompt: Option<&str>,
    default_prompt_zh: Option<&str>,
    default_prompt_en: Option<&str>,
    legacy_summary_prompt: &str,
) {
    *prompt_language = normalize_prompt_language(prompt_language.as_str()).to_string();

    let legacy_active = normalize_prompt_text(active_prompt.take())
        .filter(|value| value != legacy_summary_prompt)
        .or_else(|| normalize_optional_str(default_active_prompt));
    let mut normalized_zh = normalize_prompt_text(prompt_zh.take());
    let mut normalized_en = normalize_prompt_text(prompt_en.take());

    if normalized_zh.is_none() {
        normalized_zh = legacy_active
            .clone()
            .or_else(|| normalize_optional_str(default_prompt_zh));
    }

    if normalized_en.is_none() {
        normalized_en = normalize_optional_str(default_prompt_en);
    }

    if normalized_zh.is_none() && normalized_en.is_none() {
        normalized_zh = normalize_optional_str(default_prompt_zh);
        normalized_en = normalize_optional_str(default_prompt_en);
    }

    if *prompt_language == PROMPT_LANGUAGE_ZH && normalized_zh.is_none() && normalized_en.is_some()
    {
        *prompt_language = PROMPT_LANGUAGE_EN.to_string();
    }

    if *prompt_language == PROMPT_LANGUAGE_EN && normalized_en.is_none() && normalized_zh.is_some()
    {
        *prompt_language = PROMPT_LANGUAGE_ZH.to_string();
    }

    let effective = if *prompt_language == PROMPT_LANGUAGE_EN {
        normalized_en.clone().or_else(|| normalized_zh.clone())
    } else {
        normalized_zh.clone().or_else(|| normalized_en.clone())
    };

    *prompt_zh = normalized_zh;
    *prompt_en = normalized_en;
    *active_prompt = effective.or_else(|| normalize_optional_str(default_active_prompt));
}

fn normalize_prompt_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn normalize_optional_str(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_prompt_language(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case(PROMPT_LANGUAGE_EN) {
        PROMPT_LANGUAGE_EN
    } else {
        PROMPT_LANGUAGE_ZH
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_thread_repair_job_policy, normalize_job_policy,
        DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE, DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE_EN,
        DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE,
        DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE_EN, JOB_TYPE_THREAD_REPAIR,
    };

    #[test]
    fn thread_repair_default_policy_has_model_limits() {
        let policy = default_thread_repair_job_policy();

        assert_eq!(policy.token_limit, Some(200000));
        assert_eq!(policy.target_summary_tokens, None);
    }

    #[test]
    fn normalize_thread_repair_preserves_model_limits() {
        let mut policy = default_thread_repair_job_policy();
        policy.job_type = JOB_TYPE_THREAD_REPAIR.to_string();
        policy.model_profile_id = Some("model-a".to_string());
        policy.token_limit = Some(4096);
        policy.target_summary_tokens = Some(512);

        normalize_job_policy(&mut policy);

        assert_eq!(policy.model_profile_id.as_deref(), Some("model-a"));
        assert_eq!(policy.token_limit, Some(4096));
        assert_eq!(policy.target_summary_tokens, None);
    }

    #[test]
    fn normalize_thread_repair_fills_missing_model_limits() {
        let mut policy = default_thread_repair_job_policy();
        policy.token_limit = None;
        policy.target_summary_tokens = None;

        normalize_job_policy(&mut policy);

        assert_eq!(policy.token_limit, Some(200000));
        assert_eq!(policy.target_summary_tokens, None);
    }

    #[test]
    fn normalize_thread_repair_replaces_legacy_summary_prompt() {
        let mut policy = default_thread_repair_job_policy();
        policy.summary_prompt = Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string());
        policy.summary_prompt_zh = Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE.to_string());
        policy.summary_prompt_en = Some(DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE_EN.to_string());

        normalize_job_policy(&mut policy);

        assert_eq!(
            policy.summary_prompt.as_deref(),
            Some(DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE)
        );
        assert_eq!(
            policy.summary_prompt_zh.as_deref(),
            Some(DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE)
        );
        assert_eq!(
            policy.summary_prompt_en.as_deref(),
            Some(DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE_EN)
        );
    }
}
