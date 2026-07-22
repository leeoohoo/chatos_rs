// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_descriptor_for_record, SystemMcpHost};
use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, SystemAgentKey, SystemMcpKey};

use crate::local_runtime::storage::{LocalDatabase, LocalRuntimeSettingsRecord};
use crate::mcp::manifest::LocalMcpManifestRecord;
use crate::relay::RelayRequest;
use crate::skills::{prepare_local_skill, PreparedLocalSkill};
use crate::LocalState;

use super::prompt::compose_capability_prompt;
use super::selection::{effective_skills, filter_builtin_kinds, filter_manifests, parse_ids};

pub(crate) struct ResolvedLocalChatCapabilities {
    pub(crate) builtin_kinds: Vec<BuiltinMcpKind>,
    pub(crate) host_system_mcps: Vec<SystemMcpKey>,
    pub(crate) user_manifests: Vec<LocalMcpManifestRecord>,
    pub(crate) skills: Vec<PreparedLocalSkill>,
    pub(crate) prompt: Option<String>,
}

pub(crate) struct LocalCapabilityResolver<'a> {
    database: &'a LocalDatabase,
    owner_user_id: &'a str,
}

impl<'a> LocalCapabilityResolver<'a> {
    pub(crate) fn new(database: &'a LocalDatabase, owner_user_id: &'a str) -> Self {
        Self {
            database,
            owner_user_id,
        }
    }

    pub(crate) async fn resolve_agent(
        &self,
        agent_key: SystemAgentKey,
    ) -> Result<ResolvedAgentCapabilities, String> {
        self.database
            .get_capability_snapshot(self.owner_user_id, agent_key.as_str())
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "Plugin capability snapshot is missing for {}; connect once to sync it",
                    agent_key.as_str()
                )
            })
    }
}

pub(crate) async fn resolve_local_chat_capabilities(
    database: &LocalDatabase,
    owner_user_id: &str,
    settings: &LocalRuntimeSettingsRecord,
    state: &LocalState,
    request: &RelayRequest,
    agent_key: SystemAgentKey,
    include_all_configured: bool,
    manifest_candidates: Vec<LocalMcpManifestRecord>,
) -> Result<ResolvedLocalChatCapabilities, String> {
    let resolver = LocalCapabilityResolver::new(database, owner_user_id);
    let capabilities = resolver.resolve_agent(agent_key).await?;
    validate_primary(&capabilities)?;
    validate_required_system_mcps_for_local_connector(&capabilities)?;

    let selected_mcp_ids = if include_all_configured {
        capabilities
            .selectable_mcps()
            .map(|item| item.resource.id.clone())
            .collect::<Vec<_>>()
    } else if settings.mcp_enabled {
        normalize_selected_mcp_ids(
            parse_ids(settings.enabled_mcp_ids_json.as_str()),
            manifest_candidates.as_slice(),
        )
    } else {
        Vec::new()
    };
    let selected_optional_mcp_ids = selected_optional_mcp_ids(&capabilities, &selected_mcp_ids)?;
    let effective_mcp_ids =
        capabilities.effective_mcp_ids(selected_optional_mcp_ids.iter().map(String::as_str));
    let effective_system_descriptors = capabilities
        .mcps
        .iter()
        .filter(|item| effective_mcp_ids.contains(&item.resource.id))
        .filter_map(|item| system_mcp_descriptor_for_record(&item.resource))
        .filter(|descriptor| descriptor.supports_host(SystemMcpHost::LocalConnector))
        .collect::<Vec<_>>();
    let builtin_candidates = effective_system_descriptors
        .iter()
        .filter_map(|descriptor| descriptor.embedded_kind)
        .collect::<Vec<_>>();
    let host_system_mcps = effective_system_descriptors
        .iter()
        .filter(|descriptor| descriptor.embedded_kind.is_none())
        .map(|descriptor| descriptor.key)
        .collect::<Vec<_>>();
    let builtin_kinds = filter_builtin_kinds(&capabilities, builtin_candidates, true)?;
    validate_builtin_dependencies(agent_key, builtin_kinds.as_slice())?;
    let selected_manifests = manifest_candidates
        .into_iter()
        .filter(|manifest| {
            manifest
                .plugin_mcp_id
                .as_ref()
                .is_some_and(|resource_id| effective_mcp_ids.contains(resource_id))
        })
        .collect::<Vec<_>>();
    let user_manifests = filter_manifests(&capabilities, selected_manifests, true)?;
    if !include_all_configured {
        validate_explicit_local_mcp_selection(
            selected_optional_mcp_ids.as_slice(),
            builtin_kinds.as_slice(),
            host_system_mcps.as_slice(),
            user_manifests.as_slice(),
        )?;
    }

    let selected_skill_ids = if include_all_configured {
        capabilities
            .selectable_skills()
            .map(|item| item.resource.id.clone())
            .collect::<Vec<_>>()
    } else {
        parse_ids(settings.selected_skill_ids_json.as_str())
    };
    let mut skills = Vec::new();
    for skill in effective_skills(&capabilities, selected_skill_ids.as_slice())? {
        skills.push(prepare_local_skill(skill, state, request)?);
    }
    let effective_mcp_ids = builtin_kinds
        .iter()
        .filter_map(|kind| kind.config_id().map(str::to_string))
        .chain(host_system_mcps.iter().map(|key| {
            chatos_mcp::system_mcp_descriptor(*key)
                .resource_id
                .to_string()
        }))
        .chain(
            user_manifests
                .iter()
                .filter_map(|manifest| manifest.plugin_mcp_id.clone()),
        )
        .collect::<Vec<_>>();
    let provider_prompt = capabilities.compose_provider_skills_prompt(
        effective_mcp_ids.iter().map(String::as_str),
        Some("zh-CN"),
    );
    Ok(ResolvedLocalChatCapabilities {
        builtin_kinds,
        host_system_mcps,
        user_manifests,
        prompt: compose_capability_prompt(provider_prompt, skills.as_slice()),
        skills,
    })
}

fn normalize_selected_mcp_ids(
    selected_ids: Vec<String>,
    manifests: &[LocalMcpManifestRecord],
) -> Vec<String> {
    selected_ids
        .into_iter()
        .map(|selected_id| {
            manifests
                .iter()
                .find(|manifest| manifest.manifest_id == selected_id)
                .and_then(|manifest| manifest.plugin_mcp_id.clone())
                .unwrap_or(selected_id)
        })
        .fold(Vec::new(), |mut values, value| {
            if !values.contains(&value) {
                values.push(value);
            }
            values
        })
}

fn selected_optional_mcp_ids(
    capabilities: &ResolvedAgentCapabilities,
    selected_ids: &[String],
) -> Result<Vec<String>, String> {
    let mut optional = Vec::new();
    for selected_id in selected_ids {
        if capabilities
            .required_mcps()
            .any(|item| item.resource.id == *selected_id)
        {
            continue;
        }
        let item = capabilities
            .selectable_mcps()
            .find(|item| item.resource.id == *selected_id)
            .ok_or_else(|| {
                format!(
                    "Plugin policy does not allow MCP {} for {}",
                    selected_id, capabilities.agent_key
                )
            })?;
        if !item.binding.enabled || !item.resource.enabled || !item.available {
            return Err(format!(
                "Plugin policy MCP is unavailable for {}: {}",
                capabilities.agent_key, selected_id
            ));
        }
        optional.push(selected_id.clone());
    }
    Ok(optional)
}

fn validate_explicit_local_mcp_selection(
    selected_ids: &[String],
    builtin_kinds: &[BuiltinMcpKind],
    host_system_mcps: &[SystemMcpKey],
    manifests: &[LocalMcpManifestRecord],
) -> Result<(), String> {
    for selected_id in selected_ids {
        let builtin_match = builtin_kinds.iter().any(|kind| {
            kind.config_id() == Some(selected_id.as_str())
                || kind.kind_name() == selected_id
                || kind.server_name() == selected_id
        });
        let manifest_match = manifests
            .iter()
            .any(|manifest| manifest.plugin_mcp_id.as_deref() == Some(selected_id.as_str()));
        let host_system_match = host_system_mcps.iter().any(|key| {
            let descriptor = chatos_mcp::system_mcp_descriptor(*key);
            descriptor.resource_id == selected_id
                || descriptor.server_name == selected_id
                || key.as_str() == selected_id
        });
        if !builtin_match && !host_system_match && !manifest_match {
            return Err(format!(
                "Configured MCP is not executable by Local Connector: {selected_id}"
            ));
        }
    }
    Ok(())
}

fn validate_builtin_dependencies(
    agent_key: SystemAgentKey,
    builtin_kinds: &[BuiltinMcpKind],
) -> Result<(), String> {
    if builtin_kinds.contains(&BuiltinMcpKind::CodeMaintainerWrite)
        && !builtin_kinds.contains(&BuiltinMcpKind::CodeMaintainerRead)
    {
        return Err(format!(
            "Plugin policy for {agent_key} configures CodeMaintainerWrite without its required CodeMaintainerRead dependency"
        ));
    }
    Ok(())
}

fn validate_primary(capabilities: &ResolvedAgentCapabilities) -> Result<(), String> {
    if !capabilities.agent_enabled {
        return Err(format!(
            "Agent capability is disabled by Plugin Management: {}",
            capabilities.agent_key
        ));
    }
    capabilities
        .ensure_required_available()
        .map_err(|error| error.to_string())
}

fn validate_required_system_mcps_for_local_connector(
    capabilities: &ResolvedAgentCapabilities,
) -> Result<(), String> {
    for item in capabilities.required_mcps() {
        let Some(descriptor) = system_mcp_descriptor_for_record(&item.resource) else {
            continue;
        };
        if !descriptor.supports_host(SystemMcpHost::LocalConnector) {
            return Err(format!(
                "Plugin policy requires system MCP {} but it has no local runtime provider",
                item.resource.id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_builtin_dependencies, validate_required_system_mcps_for_local_connector};
    use chatos_mcp_runtime::BuiltinMcpKind;
    use chatos_plugin_management_sdk::{
        AgentBindingRecord, BindingConditions, McpRecord, McpRuntime, ResolvedAgentCapabilities,
        ResolvedMcp, ResourceMetadata, ResourceSecurity, SystemAgentKey,
    };

    #[test]
    fn write_capability_cannot_materialize_an_unconfigured_read_dependency() {
        let error = validate_builtin_dependencies(
            SystemAgentKey::TaskRunnerRunPhase,
            &[BuiltinMcpKind::CodeMaintainerWrite],
        )
        .expect_err("write-only Plugin policy must fail closed");
        assert!(error.contains("without its required CodeMaintainerRead dependency"));

        validate_builtin_dependencies(
            SystemAgentKey::TaskRunnerRunPhase,
            &[
                BuiltinMcpKind::CodeMaintainerRead,
                BuiltinMcpKind::CodeMaintainerWrite,
            ],
        )
        .expect("explicit read and write configuration is valid");
    }

    #[test]
    fn cloud_only_required_system_mcp_cannot_be_silently_dropped_locally() {
        let resource_id = chatos_plugin_management_sdk::PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID;
        let capabilities = ResolvedAgentCapabilities {
            agent_key: SystemAgentKey::ChatosConversationAgent.as_str().to_string(),
            owner_user_id: "user-1".to_string(),
            policy_revision: "revision".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps: vec![ResolvedMcp {
                resource: McpRecord {
                    id: resource_id.to_string(),
                    owner_user_id: "system".to_string(),
                    owner_kind: "system".to_string(),
                    visibility: "system_private".to_string(),
                    source_kind: "system_seed".to_string(),
                    name: "project_runtime_environment".to_string(),
                    display_name: "Project Runtime Environment".to_string(),
                    description: None,
                    enabled: true,
                    runtime: McpRuntime {
                        kind: "system".to_string(),
                        system_key: Some("project_runtime_environment".to_string()),
                        server_name: Some("project_runtime_environment".to_string()),
                        ..McpRuntime::default()
                    },
                    security: ResourceSecurity::default(),
                    metadata: ResourceMetadata::default(),
                    created_by: "system".to_string(),
                    updated_by: "system".to_string(),
                    created_at: "now".to_string(),
                    updated_at: "now".to_string(),
                },
                binding: AgentBindingRecord {
                    id: "binding".to_string(),
                    agent_key: SystemAgentKey::ChatosConversationAgent.as_str().to_string(),
                    binding_scope: "system_required".to_string(),
                    owner_user_id: None,
                    resource_kind: "mcp".to_string(),
                    resource_id: resource_id.to_string(),
                    enabled: true,
                    required: true,
                    priority: 10,
                    conditions: BindingConditions::default(),
                    created_by: "system".to_string(),
                    updated_by: "system".to_string(),
                    created_at: "now".to_string(),
                    updated_at: "now".to_string(),
                },
                available: true,
                status: "available".to_string(),
                reason: None,
            }],
            skills: Vec::new(),
            local_connector_requirements: Vec::new(),
        };

        let error = validate_required_system_mcps_for_local_connector(&capabilities)
            .expect_err("cloud-only MCP must fail closed in local runtime");
        assert!(error.contains("no local runtime provider"));
    }
}
