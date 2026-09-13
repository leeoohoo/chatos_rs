// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemAgentKey;

pub(crate) const SUMMARY_JOB: MemoryAiJobKind = MemoryAiJobKind::Summary;
pub(crate) const ROLLUP_JOB: MemoryAiJobKind = MemoryAiJobKind::Rollup;
pub(crate) const SUBJECT_MEMORY_JOB: MemoryAiJobKind = MemoryAiJobKind::SubjectMemory;
pub(crate) const MEMORY_ROLLUP_JOB: MemoryAiJobKind = MemoryAiJobKind::MemoryRollup;
pub(crate) const THREAD_REPAIR_JOB: MemoryAiJobKind = MemoryAiJobKind::ThreadRepair;

/// Identifies one of Memory Engine's direct, tool-free model jobs.
///
/// This is deliberately not an Agent Loop abstraction. Memory Engine owns the
/// complete request lifecycle and only reuses the published prompt key managed
/// by Plugin Management.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryAiJobKind {
    Summary,
    Rollup,
    SubjectMemory,
    MemoryRollup,
    ThreadRepair,
}

impl MemoryAiJobKind {
    pub(crate) const fn prompt_key(self) -> SystemAgentKey {
        match self {
            Self::Summary => SystemAgentKey::MemoryEngineSummaryAgent,
            Self::Rollup => SystemAgentKey::MemoryEngineRollupAgent,
            Self::SubjectMemory => SystemAgentKey::MemoryEngineSubjectMemoryAgent,
            Self::MemoryRollup => SystemAgentKey::MemoryEngineMemoryRollupAgent,
            Self::ThreadRepair => SystemAgentKey::MemoryEngineThreadRepairAgent,
        }
    }

    pub(crate) const fn job_type(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Rollup => "rollup",
            Self::SubjectMemory | Self::MemoryRollup => "subject_memory",
            Self::ThreadRepair => "thread_repair",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_jobs_keep_distinct_published_prompt_keys() {
        let jobs = [
            SUMMARY_JOB,
            ROLLUP_JOB,
            SUBJECT_MEMORY_JOB,
            MEMORY_ROLLUP_JOB,
            THREAD_REPAIR_JOB,
        ];
        let keys = jobs
            .into_iter()
            .map(|job| job.prompt_key().as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec![
                "memory_engine_summary_agent",
                "memory_engine_rollup_agent",
                "memory_engine_subject_memory_agent",
                "memory_engine_memory_rollup_agent",
                "memory_engine_thread_repair_agent",
            ]
        );
        assert_eq!(SUBJECT_MEMORY_JOB.job_type(), "subject_memory");
        assert_eq!(MEMORY_ROLLUP_JOB.job_type(), "subject_memory");
    }
}
