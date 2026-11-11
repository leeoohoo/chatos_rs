// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod agent_directory;
pub mod cloud_agent;
pub mod control_plane;
pub(crate) mod derived_events;
pub(crate) mod erasure;
pub(crate) mod generation_commits;
pub(crate) mod generation_output_commits;
pub(crate) mod history_migrations;
pub(crate) mod lineage;
pub(crate) mod memory_facts;
pub mod observability;
pub(crate) mod partition_rebuilds;
pub(crate) mod postgres;
pub mod records;
pub mod sources;
pub mod subject_memories;
pub mod subject_memory_scopes;
pub mod subjects;
pub mod summaries;
pub(crate) mod thread_partitions;
pub mod thread_snapshots;
pub mod threads;

#[cfg(test)]
mod postgres_contract_tests;
