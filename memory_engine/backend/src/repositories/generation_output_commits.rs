// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use sqlx::types::Json;
use sqlx::{Postgres, Transaction};

use crate::db::Db;
use crate::models::{
    now_rfc3339, EngineJobRun, EngineRecord, EngineSubjectMemory, EngineSummary,
    FinishEngineJobRunRequest, UpsertSubjectMemoryRequest,
};
use crate::repositories::derived_events;
use crate::repositories::generation_commits::{self, GenerationCommit};
use crate::repositories::lineage;
use crate::repositories::memory_facts::{self, FactOutputScope};
use crate::repositories::postgres::{decode, json, timestamp};
use crate::repositories::subject_memories::common::{
    build_subject_memory, lock_subject_memory_logical_key,
};
use crate::repositories::summaries::common::new_summary;

pub struct SubjectMemoryGenerationOutput<'a> {
    pub subject_id: &'a str,
    pub memory_key: &'a str,
    pub request: UpsertSubjectMemoryRequest,
    pub source_digest: Option<String>,
    pub rollup_status: &'a str,
    pub generation_profile: &'a str,
    pub project_partition_key: Option<&'a str>,
    pub payload_schema: &'a str,
    pub structured_payload: &'a serde_json::Value,
    pub pipeline_revision: &'a str,
}

pub struct ThreadSummaryGenerationOutput<'a> {
    pub thread_id: &'a str,
    pub subject_id: &'a str,
    pub summary_type: &'a str,
    pub level: i64,
    pub source_digest: Option<String>,
    pub summary_text: &'a str,
    pub source_record_start_id: Option<String>,
    pub source_record_end_id: Option<String>,
    pub source_record_count: usize,
    pub metadata: Option<serde_json::Value>,
    pub generation_reason: &'a str,
    pub generation_profile: &'a str,
    pub project_partition_key: &'a str,
    pub payload_schema: &'a str,
    pub structured_payload: &'a serde_json::Value,
    pub pipeline_revision: &'a str,
}

pub struct ThreadL0GenerationFinalization<'a> {
    pub record_ids: &'a [String],
    pub job_result: FinishEngineJobRunRequest,
}

pub struct ThreadL0ResummarizationFinalization<'a> {
    pub previous_summary_id: &'a str,
    pub record_ids: &'a [String],
    pub job_result: FinishEngineJobRunRequest,
}

pub struct ThreadRollupGenerationFinalization<'a> {
    pub summary_ids: &'a [String],
    pub job_result: FinishEngineJobRunRequest,
}

pub enum SubjectMemoryGenerationSources<'a> {
    Summaries {
        summary_ids: &'a [String],
        scope_key: Option<&'a str>,
    },
    SubjectMemories {
        memory_ids: &'a [String],
    },
}

pub struct SubjectMemoryScopeClaim<'a> {
    pub scope_key: &'a str,
    pub lock_owner: &'a str,
}

pub struct SubjectMemoryGenerationFinalization<'a> {
    pub sources: SubjectMemoryGenerationSources<'a>,
    pub scope_claim: Option<SubjectMemoryScopeClaim<'a>>,
    pub job_result: FinishEngineJobRunRequest,
}

include!("generation_output_commits/thread_commit.rs");
include!("generation_output_commits/subject_commit.rs");
include!("generation_output_commits/subject_finalization.rs");
include!("generation_output_commits/thread_l0_finalization.rs");
include!("generation_output_commits/thread_rollup_finalization.rs");
include!("generation_output_commits/shared.rs");
