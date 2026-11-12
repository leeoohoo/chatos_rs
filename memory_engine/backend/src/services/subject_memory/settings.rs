// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::models::{EngineJobPolicy, RunSubjectMemoryJobRequest};

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
        token_limit: policy.max_model_input_tokens,
        source_batch_token_limit: policy.max_model_input_tokens,
        memory_metadata: req.memory_metadata.clone(),
    })
}

pub(crate) async fn build_settings_with_policy(
    config: &AppConfig,
    req: &RunSubjectMemoryJobRequest,
) -> Result<SubjectMemoryJobSettings, String> {
    let policy =
        crate::services::control_plane::get_effective_job_policy(config, "subject_memory")?;
    build_settings(req, &policy)
}
