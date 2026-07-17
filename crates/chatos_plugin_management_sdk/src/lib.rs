// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod agent_prompts;
pub mod cache;
pub mod client;
pub mod config;
pub mod dto;
pub mod error;
pub mod policy;
pub mod provider_skills;

pub use agent_prompts::{
    agent_prompt_checksum, normalize_agent_prompt_vendor, required_agent_prompt_vendor,
    validate_agent_prompt_checksum, AgentPromptResolutionError, AgentPromptVendor,
};
pub use cache::{CapabilityCache, CapabilityCacheKey, ResolveAuthMode};
pub use client::PluginManagementClient;
pub use config::PluginManagementClientConfig;
pub use dto::*;
pub use error::{PluginManagementClientError, PolicyError};
pub use provider_skills::{
    compose_mcp_provider_skills_prompt, provider_skills_from_metadata, McpProviderSkill,
    PROVIDER_SKILLS_METADATA_KEY,
};
