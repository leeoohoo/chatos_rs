// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod provider;
mod registry;
mod store;

pub(crate) use provider::LocalAskUserProvider;
pub(crate) use registry::LocalAskUserPromptRegistry;

#[cfg(test)]
mod tests;
