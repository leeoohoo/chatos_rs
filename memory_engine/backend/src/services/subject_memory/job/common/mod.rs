// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod generated;
mod job_run;
mod prepare;
mod state;

pub(crate) use generated::generated_subject_memory_request;
pub(crate) use job_run::{
    build_outer_failed_job_run, build_success_job_run, create_subject_memory_job_run,
    finish_subject_memory_job_run,
};
pub(crate) use prepare::{empty_subject_memory_response, log_noop, prepare_subject_memory_job};
pub(crate) use state::SubjectMemoryJobProgress;

pub(crate) const SUBJECT_MEMORY_JOB_TYPE: &str = "subject_memory";
