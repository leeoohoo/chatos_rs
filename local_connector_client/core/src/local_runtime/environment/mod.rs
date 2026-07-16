// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod json_output;
mod models;
pub(crate) mod prompt;
mod registry;
mod runner;
mod scan;

#[cfg(test)]
pub(crate) use models::LocalEnvironmentImagePlan;
pub(crate) use models::{
    LocalEnvironmentAnalysisResult, LocalEnvironmentProgressRecord,
    LocalRuntimeEnvironmentImageRecord, LocalRuntimeEnvironmentRecord,
};
pub(crate) use registry::LocalEnvironmentJobRegistry;
pub(crate) use runner::run_local_environment_analysis;
