// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::{info, warn};

use crate::config::AppConfig;
use crate::db::Db;
use crate::services::ai_pipeline::summary_pipeline::{
    SummaryPipelineSpec, SummaryPipelineState, SummaryPipelineStep,
};
use crate::services::memory_ai_job::MemoryAiJobKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SummaryGenerationLease {
    ThreadSummary {
        tenant_id: String,
        source_id: String,
        thread_id: String,
        job_run_id: String,
    },
    SubjectMemory {
        tenant_id: String,
        source_id: String,
        scope_key: String,
        lock_owner: String,
    },
}

impl SummaryGenerationLease {
    async fn refresh(&self, config: &AppConfig, db: &Db) -> Result<(), String> {
        match self {
            Self::ThreadSummary {
                tenant_id,
                source_id,
                thread_id,
                job_run_id,
            } => {
                let lock_timeout_secs = config
                    .ai_request_timeout_secs
                    .saturating_mul(2)
                    .saturating_add(60);
                let refreshed = crate::repositories::threads::refresh_summary_slot(
                    db,
                    tenant_id,
                    source_id,
                    thread_id,
                    job_run_id,
                    i64::try_from(lock_timeout_secs).unwrap_or(i64::MAX),
                )
                .await?;
                if !refreshed {
                    return Err(
                        "summary slot ownership was lost during model generation".to_string()
                    );
                }
            }
            Self::SubjectMemory {
                tenant_id,
                source_id,
                scope_key,
                lock_owner,
            } => {
                let refreshed =
                    crate::repositories::subject_memory_scopes::refresh_subject_memory_scope_slot(
                        db,
                        tenant_id,
                        source_id,
                        scope_key,
                        lock_owner,
                        subject_scope_model_lock_timeout_secs(config),
                    )
                    .await?;
                if !refreshed {
                    return Err(
                        "subject memory scope ownership was lost during model generation"
                            .to_string(),
                    );
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn subject_scope_model_lock_timeout_secs(config: &AppConfig) -> i64 {
    let model_window = config
        .ai_request_timeout_secs
        .saturating_mul(2)
        .saturating_add(60);
    config
        .subject_memory_lock_timeout_secs
        .max(i64::try_from(model_window).unwrap_or(i64::MAX))
}

pub(crate) async fn generate_summary(
    config: &AppConfig,
    db: &Db,
    job: MemoryAiJobKind,
    owner_user_id: &str,
    spec: SummaryPipelineSpec,
    lease: Option<&SummaryGenerationLease>,
) -> Result<crate::services::ai_pipeline::SummaryBuildResult, String> {
    let runtime = crate::services::memory_model_runtime::build_memory_model_job_runtime(
        config,
        job,
        owner_user_id,
    )
    .await?;
    if !runtime.ai.is_enabled() {
        return Err(format!(
            "{} model is not configured or enabled",
            job.job_type()
        ));
    }

    let label = spec.log_label.clone();
    let mut pipeline = SummaryPipelineState::new(SummaryPipelineSpec {
        summary_prompt: Some(runtime.prompt),
        ..spec
    })?;
    let mut model_attempt = 1usize;

    info!(
        agent_key = job.prompt_key().as_str(),
        job_type = job.job_type(),
        model_config_id = runtime.model_config_id.as_str(),
        prompt_revision = runtime.prompt_revision,
        prompt_checksum = runtime.prompt_checksum.as_str(),
        pipeline = label.as_str(),
        "Memory Engine started direct summary generation"
    );

    loop {
        if let Some(lease) = lease {
            lease.refresh(config, db).await?;
        }

        let (step, next_pipeline) = match pipeline.execute_one(&runtime.ai, model_attempt).await {
            Ok(result) => result,
            Err(error) => {
                warn!(
                    agent_key = job.prompt_key().as_str(),
                    job_type = job.job_type(),
                    pipeline = label.as_str(),
                    model_attempt,
                    error = error.as_str(),
                    "Memory Engine direct summary generation failed"
                );
                return Err(error);
            }
        };
        pipeline = next_pipeline;
        match step {
            SummaryPipelineStep::Continue => {
                model_attempt = 1;
            }
            SummaryPipelineStep::Retry {
                error,
                retry_kind,
                next_model_attempt,
                backoff_ms,
            } => {
                warn!(
                    agent_key = job.prompt_key().as_str(),
                    job_type = job.job_type(),
                    pipeline = label.as_str(),
                    model_attempt,
                    next_model_attempt,
                    backoff_ms,
                    retry_kind = retry_kind.as_str(),
                    error = error.as_str(),
                    "Memory Engine will retry a transient model failure"
                );
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                model_attempt = next_model_attempt;
            }
            SummaryPipelineStep::Finished(result) => {
                info!(
                    agent_key = job.prompt_key().as_str(),
                    job_type = job.job_type(),
                    pipeline = label.as_str(),
                    chunk_count = result.chunk_count,
                    overflow_retry_count = result.overflow_retry_count,
                    "Memory Engine completed direct summary generation"
                );
                return Ok(result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SummaryGenerationLease;

    #[test]
    fn summary_lease_is_explicitly_scoped() {
        let lease = SummaryGenerationLease::ThreadSummary {
            tenant_id: "tenant".to_string(),
            source_id: "source".to_string(),
            thread_id: "thread".to_string(),
            job_run_id: "job".to_string(),
        };
        assert!(matches!(
            lease,
            SummaryGenerationLease::ThreadSummary { ref job_run_id, .. } if job_run_id == "job"
        ));
    }
}
