// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{EngineJobPolicy, RunSubjectMemoryJobRequest};
use crate::repositories::control_plane as cp_repo;

use super::SubjectMemoryJobSettings;

fn build_settings(
    req: &RunSubjectMemoryJobRequest,
    policy: &EngineJobPolicy,
) -> Result<SubjectMemoryJobSettings, String> {
    let subject_id = req.subject_id.trim();
    if subject_id.is_empty() {
        return Err("empty subject_id".to_string());
    }
    let memory_type = req.memory_type.trim();
    if memory_type.is_empty() {
        return Err("empty memory_type".to_string());
    }
    let thread_label = req.source_thread_label.trim();
    if thread_label.is_empty() {
        return Err("empty source_thread_label".to_string());
    }

    let configured = |name: &str, request_value: Option<i64>, policy_value: Option<i64>| {
        request_value.or(policy_value).ok_or_else(|| {
            format!("subject_memory job policy {name} is not configured in Configuration Center")
        })
    };

    Ok(SubjectMemoryJobSettings {
        relation_subject_id: req
            .relation_subject_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| subject_id.to_string()),
        source_summary_type: req
            .source_summary_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "thread_incremental".to_string()),
        prompt_title: req
            .prompt_title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("Subject memory {}", req.subject_id)),
        token_limit: configured("token_limit", req.token_limit, policy.token_limit)?.max(500),
        count_limit: configured("count_limit", req.count_limit, policy.count_limit)?.max(0),
        keep_level0_count: configured(
            "keep_level0_count",
            req.keep_level0_count,
            policy.keep_level0_count,
        )?
        .max(0),
        max_level: configured("max_level", req.max_level, policy.max_level)?.max(1),
        memory_metadata: req.memory_metadata.clone(),
    })
}

pub(crate) async fn build_settings_with_policy(
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
) -> Result<SubjectMemoryJobSettings, String> {
    let policy = cp_repo::get_effective_job_policy(db, "subject_memory").await?;
    build_settings(req, &policy)
}
