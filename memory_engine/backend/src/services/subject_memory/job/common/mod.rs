mod job_run;
mod prepare;
mod state;

pub(crate) use job_run::{
    build_failed_job_run, build_outer_failed_job_run, build_success_job_run,
    create_subject_memory_job_run, finish_subject_memory_job_run,
};
pub(crate) use prepare::{empty_subject_memory_response, log_noop, prepare_subject_memory_job};
pub(crate) use state::SubjectMemoryJobProgress;

pub(crate) const SUBJECT_MEMORY_JOB_TYPE: &str = "subject_memory";
pub(crate) const SUBJECT_MEMORY_COMPAT_JOB_TYPE: &str = "agent_memory";
pub(crate) const SUBJECT_MEMORY_SCOPE_TRIGGER: &str = "subject_scope_runner";
pub(crate) const SUBJECT_MEMORY_MANUAL_TRIGGER: &str = "manual_subject_memory";
