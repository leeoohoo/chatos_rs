// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
use chatos_agent::{
    AGENT_MAX_ITERATIONS_CONFIG_KEY, DEFAULT_AGENT_MAX_ITERATIONS,
    DEFAULT_TASK_RUNNER_PROMPT_CACHE_ENABLED, DEFAULT_TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED,
};
pub use chatos_agent::{
    TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY, TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY,
    TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
    TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY,
    TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY,
    TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY,
};
#[cfg(test)]
#[cfg(test)]
use serde_json::{json, Value};

#[path = "catalog/builtin.rs"]
mod builtin;
#[path = "catalog/constants.rs"]
mod constants;

pub use builtin::builtin_definitions;
pub use constants::*;

#[cfg(test)]
mod tests;
