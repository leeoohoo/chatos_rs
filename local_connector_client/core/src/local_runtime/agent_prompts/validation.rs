// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{bail, Result};
use chatos_plugin_management_sdk::{
    validate_agent_prompt_checksum, AgentPromptBundle, AgentPromptVendor, SystemAgentKey,
};

const MAX_AGENT_PROMPT_BYTES: usize = 64 * 1024;

pub(super) fn validate_bundle(bundle: &AgentPromptBundle) -> Result<()> {
    if bundle.bundle_version <= 0 {
        bail!("Agent Prompt Bundle version is invalid");
    }
    let expected = SystemAgentKey::ALL
        .into_iter()
        .flat_map(|agent| {
            AgentPromptVendor::ALL
                .into_iter()
                .map(move |vendor| (agent.as_str().to_string(), vendor))
        })
        .collect::<HashSet<_>>();
    let mut actual = HashSet::new();
    for prompt in &bundle.prompts {
        let key = (prompt.agent_key.trim().to_string(), prompt.vendor);
        if !expected.contains(&key) {
            bail!(
                "Agent Prompt Bundle contains unsupported entry: {} {}",
                prompt.agent_key,
                prompt.vendor
            );
        }
        if !actual.insert(key) {
            bail!(
                "Agent Prompt Bundle contains a duplicate entry: {} {}",
                prompt.agent_key,
                prompt.vendor
            );
        }
        if prompt.revision <= 0 || prompt.content.trim().is_empty() {
            bail!(
                "Agent Prompt Bundle contains an empty entry: {} {}",
                prompt.agent_key,
                prompt.vendor
            );
        }
        if prompt.content.len() > MAX_AGENT_PROMPT_BYTES {
            bail!(
                "Agent Prompt Bundle entry is too large: {} {}",
                prompt.agent_key,
                prompt.vendor
            );
        }
        if !validate_agent_prompt_checksum(prompt.content.as_str(), prompt.checksum.as_str()) {
            bail!(
                "Agent Prompt Bundle checksum is invalid: {} {}",
                prompt.agent_key,
                prompt.vendor
            );
        }
    }
    if actual != expected {
        let missing = expected.difference(&actual).count();
        bail!("Agent Prompt Bundle is incomplete: {missing} entries missing");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chatos_plugin_management_sdk::{agent_prompt_checksum, ResolvedAgentPrompt};

    use super::*;

    fn complete_bundle() -> AgentPromptBundle {
        let prompts = SystemAgentKey::ALL
            .into_iter()
            .flat_map(|agent| {
                AgentPromptVendor::ALL.into_iter().map(move |vendor| {
                    let content = format!("{} {vendor}", agent.as_str());
                    ResolvedAgentPrompt {
                        agent_key: agent.as_str().to_string(),
                        vendor,
                        checksum: agent_prompt_checksum(content.as_str()),
                        content,
                        revision: 1,
                        published_at: "2026-07-16T00:00:00Z".to_string(),
                    }
                })
            })
            .collect();
        AgentPromptBundle {
            bundle_version: 1,
            updated_at: "2026-07-16T00:00:00Z".to_string(),
            prompts,
        }
    }

    #[test]
    fn accepts_complete_checksum_valid_bundle() {
        validate_bundle(&complete_bundle()).expect("valid bundle");
    }

    #[test]
    fn rejects_partial_bundle() {
        let mut bundle = complete_bundle();
        bundle.prompts.pop();
        assert!(validate_bundle(&bundle).is_err());
    }
}
