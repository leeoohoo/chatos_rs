// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod models;
mod normalize;
mod prompt;
mod provider;
mod store;

#[cfg(test)]
mod provider_tests;

pub(crate) use models::*;
pub(crate) use normalize::*;
pub(crate) use prompt::format_local_task_board_prompt;
pub(crate) use provider::LocalTaskManagerProvider;
