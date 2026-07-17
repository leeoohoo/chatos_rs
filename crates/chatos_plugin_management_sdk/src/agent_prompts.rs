// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPromptVendor {
    Glm,
    Deepseek,
    Gpt,
    Kimi,
}

impl AgentPromptVendor {
    pub const ALL: [Self; 4] = [Self::Glm, Self::Deepseek, Self::Gpt, Self::Kimi];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Glm => "glm",
            Self::Deepseek => "deepseek",
            Self::Gpt => "gpt",
            Self::Kimi => "kimi",
        }
    }
}

impl fmt::Display for AgentPromptVendor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AgentPromptVendor {
    type Err = AgentPromptResolutionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "glm" => Ok(Self::Glm),
            "deepseek" => Ok(Self::Deepseek),
            "gpt" => Ok(Self::Gpt),
            "kimi" => Ok(Self::Kimi),
            other => Err(AgentPromptResolutionError::UnsupportedModelVendor(
                other.to_string(),
            )),
        }
    }
}

pub fn normalize_agent_prompt_vendor(
    explicit_prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Option<AgentPromptVendor> {
    if let Some(explicit) = explicit_prompt_vendor
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return AgentPromptVendor::from_str(explicit).ok();
    }
    match model_provider.trim().to_ascii_lowercase().as_str() {
        "glm" | "zhipu" | "zai" | "chatglm" => Some(AgentPromptVendor::Glm),
        "deepseek" => Some(AgentPromptVendor::Deepseek),
        "gpt" | "openai" => Some(AgentPromptVendor::Gpt),
        "kimi" | "kimik2" | "moonshot" => Some(AgentPromptVendor::Kimi),
        _ => None,
    }
}

pub fn required_agent_prompt_vendor(
    explicit_prompt_vendor: Option<&str>,
    model_provider: &str,
) -> Result<AgentPromptVendor, AgentPromptResolutionError> {
    normalize_agent_prompt_vendor(explicit_prompt_vendor, model_provider).ok_or_else(|| {
        AgentPromptResolutionError::UnsupportedModelVendor(model_provider.trim().to_string())
    })
}

pub fn agent_prompt_checksum(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

pub fn validate_agent_prompt_checksum(content: &str, checksum: &str) -> bool {
    agent_prompt_checksum(content) == checksum.trim()
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AgentPromptResolutionError {
    #[error("unsupported_model_vendor: {0}")]
    UnsupportedModelVendor(String),
    #[error("agent_prompt_not_configured")]
    NotConfigured,
    #[error("agent_prompt_disabled")]
    Disabled,
    #[error("agent_prompt_empty")]
    Empty,
    #[error("agent_prompt_checksum_invalid")]
    ChecksumInvalid,
    #[error("agent_prompt_bundle_not_initialized")]
    BundleNotInitialized,
    #[error("agent_prompt_gateway_unavailable")]
    GatewayUnavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_vendor_wins_over_provider_alias() {
        assert_eq!(
            required_agent_prompt_vendor(Some("glm"), "openai").expect("vendor"),
            AgentPromptVendor::Glm
        );
    }

    #[test]
    fn provider_aliases_map_to_fixed_prompt_vendors() {
        for (provider, expected) in [
            ("zhipu", AgentPromptVendor::Glm),
            ("deepseek", AgentPromptVendor::Deepseek),
            ("openai", AgentPromptVendor::Gpt),
            ("moonshot", AgentPromptVendor::Kimi),
        ] {
            assert_eq!(
                required_agent_prompt_vendor(None, provider).expect("vendor"),
                expected
            );
        }
    }

    #[test]
    fn checksum_round_trip_is_stable() {
        let checksum = agent_prompt_checksum("system prompt");
        assert!(validate_agent_prompt_checksum(
            "system prompt",
            checksum.as_str()
        ));
        assert!(!validate_agent_prompt_checksum(
            "changed",
            checksum.as_str()
        ));
    }
}
