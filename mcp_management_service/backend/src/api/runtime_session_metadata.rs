// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::RuntimeToolDescriptor;
use chatos_plugin_management_sdk::ResolvedAgentCapabilities;

pub(super) struct RuntimeSessionPromptMetadata {
    pub(super) effective_mcp_ids: Vec<String>,
    pub(super) provider_skills_prompt: Option<String>,
}

pub(super) fn resolve_runtime_session_prompt_metadata(
    capabilities: &ResolvedAgentCapabilities,
    tools: &[RuntimeToolDescriptor],
    locale: Option<&str>,
) -> RuntimeSessionPromptMetadata {
    let mut effective_mcp_ids = tools
        .iter()
        .map(|tool| tool.resource_id.clone())
        .collect::<Vec<_>>();
    effective_mcp_ids.sort();
    effective_mcp_ids.dedup();
    let provider_skills_prompt = capabilities.compose_provider_skills_prompt(
        effective_mcp_ids.iter().map(String::as_str),
        normalized_provider_prompt_locale(locale),
    );
    RuntimeSessionPromptMetadata {
        effective_mcp_ids,
        provider_skills_prompt,
    }
}

fn normalized_provider_prompt_locale(value: Option<&str>) -> Option<&str> {
    match value.map(str::trim) {
        Some("en-US") => Some("en-US"),
        Some("zh-CN") => Some("zh-CN"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_prompt_locale_accepts_only_supported_values() {
        assert_eq!(
            normalized_provider_prompt_locale(Some(" en-US ")),
            Some("en-US")
        );
        assert_eq!(
            normalized_provider_prompt_locale(Some("zh-CN")),
            Some("zh-CN")
        );
        assert_eq!(normalized_provider_prompt_locale(Some("en")), None);
        assert_eq!(normalized_provider_prompt_locale(None), None);
    }

    #[test]
    fn effective_mcp_ids_come_from_the_exposed_tool_snapshot() {
        let capabilities = ResolvedAgentCapabilities {
            agent_key: "agent".to_string(),
            owner_user_id: "user".to_string(),
            policy_revision: "policy".to_string(),
            generated_at: String::new(),
            agent_enabled: true,
            mcps: Vec::new(),
            skills: Vec::new(),
            plugins: Vec::new(),
            local_connector_requirements: Vec::new(),
        };
        let tools = [
            RuntimeToolDescriptor {
                exposed_name: "b_tool".to_string(),
                original_name: "tool".to_string(),
                resource_id: "mcp-b".to_string(),
                definition: json!({}),
            },
            RuntimeToolDescriptor {
                exposed_name: "a_tool".to_string(),
                original_name: "tool".to_string(),
                resource_id: "mcp-a".to_string(),
                definition: json!({}),
            },
            RuntimeToolDescriptor {
                exposed_name: "a_tool_2".to_string(),
                original_name: "tool_2".to_string(),
                resource_id: "mcp-a".to_string(),
                definition: json!({}),
            },
        ];
        let metadata =
            resolve_runtime_session_prompt_metadata(&capabilities, &tools, Some("zh-CN"));
        assert_eq!(metadata.effective_mcp_ids, ["mcp-a", "mcp-b"]);
        assert!(metadata.provider_skills_prompt.is_none());
    }
}
