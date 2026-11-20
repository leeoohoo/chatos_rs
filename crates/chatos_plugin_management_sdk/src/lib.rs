// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod agent_prompts;
pub mod client;
pub mod config;
pub mod dto;
pub mod error;
pub mod plugin_manifest;
pub mod plugin_runtime;
pub mod plugin_signing;
pub mod skill_runtime;

pub use agent_prompts::{
    agent_prompt_checksum, normalize_agent_prompt_profile, normalize_agent_prompt_vendor,
    required_agent_prompt_vendor, validate_agent_prompt_checksum, AgentPromptResolutionError,
    AgentPromptVendor, DEFAULT_AGENT_PROMPT_PROFILE,
};
pub use client::PluginManagementClient;
pub use config::PluginManagementClientConfig;
pub use dto::*;
pub use error::PluginManagementClientError;
pub use plugin_manifest::*;
pub use plugin_runtime::*;
pub use plugin_signing::*;
pub use skill_runtime::*;
