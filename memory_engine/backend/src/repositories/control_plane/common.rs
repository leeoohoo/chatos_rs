// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) const STALE_LEGACY_REPAIR_JOB_TIMEOUT_SECS: i64 = 1800;

pub(crate) const JOB_TYPE_SUMMARY: &str = "summary";
pub(crate) const JOB_TYPE_ROLLUP: &str = "rollup";
pub(crate) const JOB_TYPE_SUBJECT_MEMORY: &str = "subject_memory";
pub(crate) const LEGACY_JOB_TYPE_THREAD_REPAIR: &str = "thread_repair";

pub fn default_job_types() -> &'static [&'static str] {
    &[JOB_TYPE_SUMMARY, JOB_TYPE_ROLLUP, JOB_TYPE_SUBJECT_MEMORY]
}
