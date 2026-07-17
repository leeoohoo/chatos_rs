// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, SystemAgentKey};

use crate::local_runtime::storage::{LocalDatabase, LocalRuntimeSettingsRecord};
use crate::mcp::manifest::LocalMcpManifestRecord;
use crate::relay::RelayRequest;
use crate::skills::{prepare_local_skill, PreparedLocalSkill};
use crate::LocalState;

use super::prompt::compose_capability_prompt;
use super::selection::{
    effective_skills, filter_builtin_kinds, filter_manifests, parse_ids, validate_selected_mcp_ids,
};

pub(crate) struct ResolvedLocalChatCapabilities {
    pub(crate) builtin_kinds: Vec<BuiltinMcpKind>,
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
    builtin_candidates: Vec<BuiltinMcpKind>,
    manifest_candidates: Vec<LocalMcpManifestRecord>,
) -> Result<ResolvedLocalChatCapabilities, String> {
    let resolver = LocalCapabilityResolver::new(database, owner_user_id);
    let primary_key = if settings.plan_mode_enabled {
        SystemAgentKey::ChatosPlanningAgent
    } else {
        SystemAgentKey::ChatosConversationAgent
    };
    let primary = resolver.resolve_agent(primary_key).await?;
    validate_primary(&primary)?;
    let task_runner = resolver
        .resolve_agent(SystemAgentKey::TaskRunnerRunPhase)
        .await?;
    if !task_runner.agent_enabled {
        return Err("Task Runner capability is disabled by Plugin Management".to_string());
    }
    task_runner
        .ensure_required_available()
        .map_err(|error| error.to_string())?;

    let selected_mcp_ids = if settings.mcp_enabled && !settings.plan_mode_enabled {
        parse_ids(settings.enabled_mcp_ids_json.as_str())
    } else {
        Vec::new()
    };
    let explicit_mcp_selection = !selected_mcp_ids.is_empty();
    let builtin_kinds = filter_builtin_kinds(
        &task_runner,
        builtin_candidates,
        explicit_mcp_selection || settings.plan_mode_enabled,
    )?;
    let user_manifests =
        filter_manifests(&task_runner, manifest_candidates, explicit_mcp_selection)?;
    validate_selected_mcp_ids(
        selected_mcp_ids.as_slice(),
        builtin_kinds.as_slice(),
        user_manifests.as_slice(),
    )?;
    let selected_skill_ids = if settings.plan_mode_enabled {
        Vec::new()
    } else {
        parse_ids(settings.selected_skill_ids_json.as_str())
    };
    let mut skills = Vec::new();
    for skill in effective_skills(&task_runner, selected_skill_ids.as_slice())? {
        skills.push(prepare_local_skill(skill, state, request)?);
    }
    let effective_mcp_ids = builtin_kinds
        .iter()
        .filter_map(|kind| kind.config_id().map(str::to_string))
        .chain(
            user_manifests
                .iter()
                .filter_map(|manifest| manifest.plugin_mcp_id.clone()),
        )
        .collect::<Vec<_>>();
    let provider_prompt = task_runner.compose_provider_skills_prompt(
        effective_mcp_ids.iter().map(String::as_str),
        Some("zh-CN"),
    );
    Ok(ResolvedLocalChatCapabilities {
        builtin_kinds,
        user_manifests,
        prompt: compose_capability_prompt(provider_prompt, skills.as_slice()),
        skills,
    })
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
