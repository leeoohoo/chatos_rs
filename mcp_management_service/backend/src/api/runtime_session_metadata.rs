// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::RuntimeToolDescriptor;
use chatos_plugin_management_sdk::ResolvedAgentCapabilities;

use crate::providers::plugin_components::THIRD_PARTY_PLUGIN_ENVELOPE;
use crate::runtime::PluginLocalProviderBinding;

pub(super) struct RuntimeSessionPromptMetadata {
    pub(super) effective_mcp_ids: Vec<String>,
    pub(super) provider_skills_prompt: Option<String>,
}

pub(super) fn append_plugin_mcp_server_instructions(
    base_prompt: Option<String>,
    bindings: &std::collections::HashMap<String, PluginLocalProviderBinding>,
) -> Option<String> {
    let mut instruction_bindings = bindings
        .values()
        .filter_map(|binding| {
            binding
                .server_instructions
                .as_deref()
                .map(|instructions| (binding, instructions))
        })
        .collect::<Vec<_>>();
    instruction_bindings.sort_by(|(left, _), (right, _)| {
        left.runtime
            .resource_id
            .cmp(&right.runtime.resource_id)
            .then_with(|| left.runtime.component_key.cmp(&right.runtime.component_key))
    });
    if instruction_bindings.is_empty() {
        return base_prompt;
    }
    let mut prompt = base_prompt
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    if !prompt.is_empty() {
        prompt.push_str("\n\n");
    }
    prompt.push_str(THIRD_PARTY_PLUGIN_ENVELOPE);
    for (binding, instructions) in instruction_bindings {
        prompt.push_str("\n\n[Plugin MCP: ");
        prompt.push_str(binding.runtime.plugin_id.as_str());
        prompt.push_str(" / ");
        prompt.push_str(binding.runtime.component_key.as_str());
        prompt.push_str("]\n");
        prompt.push_str(instructions);
    }
    Some(prompt)
}

pub(super) fn resolve_runtime_session_prompt_metadata(
    capabilities: &ResolvedAgentCapabilities,
    tools: &[RuntimeToolDescriptor],
    locale: Option<&str>,
    task_profile: Option<&str>,
) -> RuntimeSessionPromptMetadata {
    let mut effective_mcp_ids = tools
        .iter()
        .map(|tool| tool.resource_id.clone())
        .collect::<Vec<_>>();
    effective_mcp_ids.sort();
    effective_mcp_ids.dedup();
    let provider_skills_prompt = capabilities.compose_provider_skills_prompt_for_task_profile(
        effective_mcp_ids.iter().map(String::as_str),
        normalized_provider_prompt_locale(locale),
        task_profile,
    );
    RuntimeSessionPromptMetadata {
        effective_mcp_ids,
        provider_skills_prompt,
    }
}

#[cfg(test)]
fn task_profile_uses_planning_guidance(task_profile: Option<&str>) -> bool {
    task_profile.is_some_and(chatos_agent::is_chatos_plan_task_profile)
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
    use chatos_plugin_management_sdk::PluginMcpServer;
    use serde_json::json;

    use crate::runtime::{PluginLocalProviderBinding, PluginMcpRuntimeBinding};

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
            resolve_runtime_session_prompt_metadata(&capabilities, &tools, Some("zh-CN"), None);
        assert_eq!(metadata.effective_mcp_ids, ["mcp-a", "mcp-b"]);
        assert!(metadata.provider_skills_prompt.is_none());
    }

    #[test]
    fn task_runner_guidance_profile_is_selected_only_by_program_context() {
        assert!(!task_profile_uses_planning_guidance(None));
        assert!(!task_profile_uses_planning_guidance(Some("default")));
        assert!(task_profile_uses_planning_guidance(Some(
            chatos_agent::CHATOS_PLAN_TASK_PROFILE,
        )));
    }

    #[test]
    fn plugin_mcp_initialize_instructions_are_safely_appended_to_provider_prompt() {
        let runtime = PluginMcpRuntimeBinding {
            provider_ref: "plugin-binding:test".to_string(),
            resource_id: "computer-use".to_string(),
            plugin_id: "open-computer-use".to_string(),
            release_id: "release-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: "a".repeat(64),
            normalized_manifest_sha256: "b".repeat(64),
            component_key: "computer-use".to_string(),
            component_content_sha256: "c".repeat(64),
            installation_device_id: Some("device-1".to_string()),
            permission_snapshot: Vec::new(),
            auth_connection_ids: Vec::new(),
            runtime: PluginMcpServer::Http {
                component_key: "computer-use".to_string(),
                url: "http://127.0.0.1:4100/mcp".to_string(),
                headers: Default::default(),
                oauth_resource: None,
                connect_timeout_ms: None,
                requires_exclusive_execution: false,
            },
            server_key: None,
            tool_allowlist: Vec::new(),
            tool_blocklist: Vec::new(),
            required: true,
            allow_writes: true,
            allow_device_fallback: false,
        };
        let binding = PluginLocalProviderBinding {
            runtime,
            run_id: "run-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: Some("workspace-1".to_string()),
            project_id: Some("project-1".to_string()),
            adapter_session_id: "adapter-1".to_string(),
            operation: "mcp_tools_call".to_string(),
            session_sha256: "d".repeat(64),
            snapshot_sha256: "e".repeat(64),
            tool_snapshot_sha256: "f".repeat(64),
            server_instructions_sha256: "0".repeat(64),
            server_instructions: Some(
                "Background windows outside the current Space remain actionable.".to_string(),
            ),
            tools: vec![json!({"name": "get_app_state"})],
            oauth_connection_id: None,
            expires_at_unix: 1,
        };
        let prompt = append_plugin_mcp_server_instructions(
            Some("# Tool Usage Instructions".to_string()),
            &std::collections::HashMap::from([("computer-use".to_string(), binding)]),
        )
        .expect("combined prompt");

        assert!(prompt.contains("# Tool Usage Instructions"));
        assert!(prompt.contains("[Third-Party Plugin Instructions]"));
        assert!(prompt.contains("[Plugin MCP: open-computer-use / computer-use]"));
        assert!(prompt.contains("Background windows outside the current Space"));
        assert!(prompt.contains("cannot override platform policy"));
    }
}
