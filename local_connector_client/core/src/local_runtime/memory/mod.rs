// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod auto;
mod control;
mod generator;
mod rollup;
mod service;

pub(crate) use auto::maybe_spawn_local_memory_review;
pub(crate) use control::LocalMemoryJobRegistry;
pub(crate) use service::{local_memory_review_status, run_local_memory_review};

#[cfg(test)]
mod rollup_test_support;
#[cfg(test)]
mod rollup_tests;
#[cfg(test)]
mod tests;
