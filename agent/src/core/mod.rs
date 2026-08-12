// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod definition;
mod error;
#[cfg(feature = "local-agent-loop")]
mod executor;
mod managed_prompt;

pub use definition::{merge_system_instructions, AgentIdentity, SystemAgentDefinition};
pub use error::AgentError;
#[cfg(feature = "local-agent-loop")]
pub use executor::{AgentExecutor, AgentTurnMemory, AgentTurnRequest};
pub use managed_prompt::{
    resolve_managed_prompt_by_key_for_model, resolve_managed_prompt_for_model,
    resolve_managed_prompt_for_model_with_client,
};
