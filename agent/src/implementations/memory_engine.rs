// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{ResolvedAgentPrompt, SystemAgentKey};

use crate::{
    agent_descriptor, resolve_managed_prompt_for_model, AgentDescriptor, AgentError, AgentIdentity,
    SystemAgentDefinition,
};

pub const MEMORY_ENGINE_SUMMARY_AGENT: MemoryEngineAgent =
    MemoryEngineAgent::new(MemoryEngineAgentKind::Summary);
pub const MEMORY_ENGINE_ROLLUP_AGENT: MemoryEngineAgent =
    MemoryEngineAgent::new(MemoryEngineAgentKind::Rollup);
pub const MEMORY_ENGINE_SUBJECT_MEMORY_AGENT: MemoryEngineAgent =
    MemoryEngineAgent::new(MemoryEngineAgentKind::SubjectMemory);
pub const MEMORY_ENGINE_MEMORY_ROLLUP_AGENT: MemoryEngineAgent =
    MemoryEngineAgent::new(MemoryEngineAgentKind::MemoryRollup);
pub const MEMORY_ENGINE_THREAD_REPAIR_AGENT: MemoryEngineAgent =
    MemoryEngineAgent::new(MemoryEngineAgentKind::ThreadRepair);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEngineAgentKind {
    Summary,
    Rollup,
    SubjectMemory,
    MemoryRollup,
    ThreadRepair,
}

impl MemoryEngineAgentKind {
    pub const fn agent_key(self) -> SystemAgentKey {
        match self {
            Self::Summary => SystemAgentKey::MemoryEngineSummaryAgent,
            Self::Rollup => SystemAgentKey::MemoryEngineRollupAgent,
            Self::SubjectMemory => SystemAgentKey::MemoryEngineSubjectMemoryAgent,
            Self::MemoryRollup => SystemAgentKey::MemoryEngineMemoryRollupAgent,
            Self::ThreadRepair => SystemAgentKey::MemoryEngineThreadRepairAgent,
        }
    }

    pub const fn job_type(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Rollup => "rollup",
            Self::SubjectMemory | Self::MemoryRollup => "subject_memory",
            Self::ThreadRepair => "thread_repair",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryEngineAgent {
    kind: MemoryEngineAgentKind,
}

impl MemoryEngineAgent {
    pub const fn new(kind: MemoryEngineAgentKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> MemoryEngineAgentKind {
        self.kind
    }

    pub const fn job_type(self) -> &'static str {
        self.kind.job_type()
    }

    pub async fn resolve_prompt(
        &self,
        model_provider: &str,
    ) -> Result<ResolvedAgentPrompt, AgentError> {
        resolve_managed_prompt_for_model("memory-engine", self, None, model_provider).await
    }
}

impl AgentIdentity for MemoryEngineAgent {
    fn descriptor(&self) -> &'static AgentDescriptor {
        agent_descriptor(self.kind.agent_key())
    }
}

impl SystemAgentDefinition for MemoryEngineAgent {
    fn message_mode(&self) -> &'static str {
        self.descriptor().key.as_str()
    }

    fn message_source(&self) -> &'static str {
        "memory_engine"
    }

    fn context_overflow_trigger(&self) -> &'static str {
        "memory_engine_context_overflow"
    }

    fn default_temperature(&self) -> Option<f64> {
        Some(0.2)
    }

    fn default_max_output_tokens(&self) -> Option<i64> {
        Some(4_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_agents_share_one_definition_and_keep_distinct_identities() {
        let agents = [
            MEMORY_ENGINE_SUMMARY_AGENT,
            MEMORY_ENGINE_ROLLUP_AGENT,
            MEMORY_ENGINE_SUBJECT_MEMORY_AGENT,
            MEMORY_ENGINE_MEMORY_ROLLUP_AGENT,
            MEMORY_ENGINE_THREAD_REPAIR_AGENT,
        ];
        let keys = agents
            .iter()
            .map(|agent| agent.descriptor().key.as_str())
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
        assert_eq!(
            MEMORY_ENGINE_SUBJECT_MEMORY_AGENT.job_type(),
            "subject_memory"
        );
        assert_eq!(
            MEMORY_ENGINE_MEMORY_ROLLUP_AGENT.job_type(),
            "subject_memory"
        );
        assert!(agents
            .iter()
            .all(|agent| agent.message_source() == "memory_engine"));
    }
}
