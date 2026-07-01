// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{Duration, Utc};
use futures_util::{stream, StreamExt};
use tracing::info;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    EngineSubjectMemoryScope, RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse,
    RunSubjectMemoryScopesResponse,
};
use crate::repositories::{control_plane as cp_repo, subject_memory_scopes};

use super::job::run_subject_memory_job_internal;

enum ScopeExecutionOutcome {
    Success(RunSubjectMemoryJobResponse),
    Failed {
        tenant_id: String,
        source_id: String,
        scope_key: String,
        error: String,
    },
}

pub async fn run_registered_subject_memory_scopes(
    config: &AppConfig,
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<RunSubjectMemoryScopesResponse, String> {
    run_registered_subject_memory_scopes_internal(config, db, tenant_id, source_id, limit, false)
        .await
}

pub async fn run_registered_subject_memory_scopes_due(
    config: &AppConfig,
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<RunSubjectMemoryScopesResponse, String> {
    run_registered_subject_memory_scopes_internal(config, db, tenant_id, source_id, limit, true)
        .await
}

async fn run_registered_subject_memory_scopes_internal(
    config: &AppConfig,
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
    respect_interval: bool,
) -> Result<RunSubjectMemoryScopesResponse, String> {
    let scopes = if respect_interval {
        let policy = cp_repo::get_effective_job_policy(db, "subject_memory").await?;
        let interval_seconds = policy.interval_seconds.unwrap_or(60).max(3);
        let ready_before = (Utc::now() - Duration::seconds(interval_seconds)).to_rfc3339();
        subject_memory_scopes::list_runnable_subject_memory_scopes(
            db,
            tenant_id,
            source_id,
            ready_before.as_str(),
            limit,
        )
        .await?
    } else {
        subject_memory_scopes::list_active_subject_memory_scopes(db, tenant_id, source_id, limit)
            .await?
    };
    if scopes.is_empty() {
        return Ok(RunSubjectMemoryScopesResponse {
            processed_scopes: 0,
            generated_scopes: 0,
            generated_memories: 0,
            marked_source_summaries: 0,
            marked_source_memories: 0,
            failed_scopes: 0,
        });
    }

    let mut out = RunSubjectMemoryScopesResponse {
        processed_scopes: scopes.len(),
        generated_scopes: 0,
        generated_memories: 0,
        marked_source_summaries: 0,
        marked_source_memories: 0,
        failed_scopes: 0,
    };

    let concurrency = subject_memory_scope_concurrency(config, limit);
    let db = db.clone();
    let config = config.clone();
    let execution_results = stream::iter(scopes.into_iter().map(|scope| {
        let db = db.clone();
        let config = config.clone();
        async move {
            let tenant_id = scope.tenant_id.clone();
            let source_id = scope.source_id.clone();
            let scope_key = scope.scope_key.clone();
            match run_scope_once(&config, &db, &scope).await {
                Ok(result) => ScopeExecutionOutcome::Success(result),
                Err(error) => ScopeExecutionOutcome::Failed {
                    tenant_id,
                    source_id,
                    scope_key,
                    error,
                },
            }
        }
    }))
    .buffer_unordered(concurrency)
    .collect::<Vec<_>>()
    .await;

    for outcome in execution_results {
        match outcome {
            ScopeExecutionOutcome::Success(result) => {
                if result.generated_memories > 0 {
                    out.generated_scopes += 1;
                }
                out.generated_memories += result.generated_memories;
                out.marked_source_summaries += result.marked_source_summaries;
                out.marked_source_memories += result.marked_source_memories;
            }
            ScopeExecutionOutcome::Failed {
                tenant_id,
                source_id,
                scope_key,
                error,
            } => {
                out.failed_scopes += 1;
                info!(
                    "[MEMORY-ENGINE-SUBJECT] scope run failed tenant_id={} source_id={} scope_key={} error={}",
                    tenant_id, source_id, scope_key, error
                );
            }
        }
    }

    Ok(out)
}

async fn run_scope_once(
    config: &AppConfig,
    db: &Db,
    scope: &EngineSubjectMemoryScope,
) -> Result<RunSubjectMemoryJobResponse, String> {
    subject_memory_scopes::touch_subject_memory_scope_run(
        db,
        scope.tenant_id.as_str(),
        scope.source_id.as_str(),
        scope.scope_key.as_str(),
    )
    .await?;

    run_subject_memory_job_internal(
        config,
        db,
        RunSubjectMemoryJobRequest {
            tenant_id: scope.tenant_id.clone(),
            source_id: scope.source_id.clone(),
            subject_id: scope.subject_id.clone(),
            memory_type: scope.memory_type.clone(),
            source_thread_label: scope.source_thread_label.clone(),
            relation_subject_id: scope.relation_subject_id.clone(),
            source_summary_type: scope.source_summary_type.clone(),
            summary_prompt: None,
            rollup_summary_prompt: None,
            prompt_title: scope.prompt_title.clone(),
            token_limit: None,
            target_summary_tokens: None,
            count_limit: None,
            keep_level0_count: None,
            max_level: None,
            memory_metadata: scope.memory_metadata.clone(),
        },
        true,
    )
    .await
}

fn subject_memory_scope_concurrency(config: &AppConfig, limit: i64) -> usize {
    limit
        .max(1)
        .min(config.worker_subject_memory_concurrency.max(1) as i64) as usize
}
