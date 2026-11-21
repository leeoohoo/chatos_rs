use super::{
    non_empty, ControlPlaneModelOption, MAIN_CHAT_INSTRUCTIONS, MAIN_CHAT_PROFILE,
    MAX_TASK_ITERATIONS, TASK_EXECUTION_INSTRUCTIONS, TASK_EXECUTION_PROFILE, TASK_POLICY_PROFILE,
    WINDOWS_CAPABILITY_SCHEMA,
};
use crate::{
    agent_plugin_runtime::capability_broker_tool_schemas,
    attachment_tool_worker::{
        attachment_read_tool_schema, project_dashboard_tool_schemas, team_asset_tool_schemas,
        team_chat_read_tool_schemas, team_chat_write_tool_schemas, todo_execution_tool_schemas,
        todo_planning_read_tool_schemas, todo_planning_write_tool_schemas,
    },
    authentication::{GatewayManagedRuntimeConfig, GatewayModelConfig},
    host_lifecycle::transport::{request, ManagedHost},
    plugin_installer_tool_worker::{
        plugin_installer_revision, plugin_installer_tool_schemas, PLUGIN_INSTALLER_INSTRUCTIONS,
    },
    terminal_tool_worker::{
        project_read_tool_schemas, project_write_tool_schemas, terminal_tool_schemas,
    },
};
use chatos_local_agent_protocol::{
    HostCommand, HostResult, LocalCapabilityPolicySnapshot, LocalModelConfigSnapshot,
    PutCapabilityPolicySnapshotCommand, PutModelConfigSnapshotCommand,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::OnceLock};

const MODEL_REASONING_CAPABILITIES: &str = include_str!(
    "../../../macos/Sources/ChatOSModelCapabilities/Resources/ModelReasoningCapabilities.json"
);

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelReasoningCapability {
    model: String,
    levels: Vec<String>,
    default_level: String,
    legacy_aliases: HashMap<String, String>,
}

#[derive(serde::Deserialize)]
struct ModelReasoningCatalog {
    models: Vec<ModelReasoningCapability>,
}

pub(crate) fn model_option_from_gateway(model: &GatewayModelConfig) -> ControlPlaneModelOption {
    model_option(
        model.id.clone(),
        non_empty(model.name.clone()).unwrap_or_else(|| model.model.clone()),
        model.model.clone(),
        model.provider.clone(),
        model.task_thinking_level.clone(),
        model.supports_reasoning,
        String::new(),
    )
}

pub(super) fn model_option_from_snapshot(
    snapshot: &LocalModelConfigSnapshot,
) -> ControlPlaneModelOption {
    model_option(
        snapshot.model_config_ref.clone(),
        snapshot.model.clone(),
        snapshot.model.clone(),
        snapshot.provider.clone(),
        snapshot.thinking_level.clone(),
        snapshot.thinking_level.is_some(),
        snapshot.model_config_revision.clone(),
    )
}

pub(super) fn model_option(
    id: String,
    display_name: String,
    model_name: String,
    provider: String,
    configured_level: Option<String>,
    advertised_reasoning: bool,
    model_config_revision: String,
) -> ControlPlaneModelOption {
    if let Some(capability) = reasoning_capability(&model_name) {
        let configured = configured_level
            .as_deref()
            .map(|level| level.trim().to_ascii_lowercase());
        let default_thinking_level = configured
            .as_deref()
            .filter(|level| {
                capability
                    .levels
                    .iter()
                    .any(|candidate| candidate == *level)
            })
            .map(str::to_owned)
            .or_else(|| {
                configured
                    .as_deref()
                    .and_then(|level| capability.legacy_aliases.get(level).cloned())
            })
            .unwrap_or_else(|| capability.default_level.clone());
        return ControlPlaneModelOption {
            id,
            display_name,
            model_name,
            provider,
            default_thinking_level,
            supports_reasoning: true,
            thinking_levels: capability.levels.clone(),
            model_config_revision,
        };
    }
    let supports_reasoning = advertised_reasoning || configured_level.is_some();
    let thinking_levels = if supports_reasoning {
        provider_thinking_levels(&provider)
    } else {
        Vec::new()
    };
    let configured = configured_level
        .as_deref()
        .map(|level| normalize_provider_level(&provider, level));
    let default_thinking_level = configured
        .filter(|level| thinking_levels.contains(level))
        .or_else(|| {
            supports_reasoning
                .then(|| default_enabled_level(&thinking_levels))
                .filter(|level| !level.is_empty())
        })
        .unwrap_or_else(|| "none".into());
    ControlPlaneModelOption {
        id,
        display_name,
        model_name,
        provider,
        default_thinking_level,
        supports_reasoning,
        thinking_levels,
        model_config_revision,
    }
}

fn reasoning_capability(model: &str) -> Option<&'static ModelReasoningCapability> {
    static CATALOG: OnceLock<ModelReasoningCatalog> = OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        serde_json::from_str(MODEL_REASONING_CAPABILITIES)
            .expect("shared model reasoning capabilities must be valid")
    });
    let model = model.trim().to_ascii_lowercase();
    catalog.models.iter().find(|capability| {
        model == capability.model
            || model
                .strip_prefix(&format!("{}-", capability.model))
                .is_some_and(|suffix| {
                    suffix.len() == 10
                        && suffix
                            .chars()
                            .all(|value| value.is_ascii_digit() || value == '-')
                })
    })
}

fn provider_thinking_levels(provider: &str) -> Vec<String> {
    let values: &[&str] = match normalized_provider(provider).as_str() {
        "gpt" => &["none", "minimal", "low", "medium", "high", "xhigh"],
        "deepseek" => &["none", "low", "medium", "high", "max"],
        "kimi" => &["none", "auto", "low", "medium", "high", "xhigh"],
        _ => &["none", "low", "medium", "high", "xhigh"],
    };
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn normalized_provider(provider: &str) -> String {
    match provider
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "openai" | "gpt" => "gpt".into(),
        "kimik2" | "kimi" | "moonshot" => "kimi".into(),
        "openai_compatible" | "compatible" => "openai_compatible".into(),
        value => value.to_owned(),
    }
}

fn normalize_provider_level(provider: &str, level: &str) -> String {
    let level = level.trim().to_ascii_lowercase();
    match level.as_str() {
        "off" | "disabled" => "none".into(),
        "max" | "xhigh" if normalized_provider(provider) == "deepseek" => "max".into(),
        "max" => "xhigh".into(),
        "minimal" if normalized_provider(provider) == "openai_compatible" => "low".into(),
        _ => level,
    }
}

fn default_enabled_level(levels: &[String]) -> String {
    for preferred in ["auto", "medium"] {
        if levels.iter().any(|level| level == preferred) {
            return preferred.into();
        }
    }
    levels
        .iter()
        .find(|level| level.as_str() != "none")
        .cloned()
        .unwrap_or_else(|| "none".into())
}

pub(super) async fn publish_model(
    host: &mut ManagedHost,
    snapshot: LocalModelConfigSnapshot,
) -> Result<(), String> {
    let result = request(
        host,
        HostCommand::PutModelConfigSnapshot(PutModelConfigSnapshotCommand { snapshot }),
    )
    .await
    .map_err(|error| error.message)?;
    if matches!(result, HostResult::ModelConfigSnapshot { .. }) {
        Ok(())
    } else {
        Err("Local Agent Host 返回了错误的模型发布响应。".into())
    }
}

pub(super) fn task_max_iterations(config: &GatewayManagedRuntimeConfig) -> Result<u32, String> {
    let configured = config
        .local_task_execution_settings
        .as_ref()
        .map(|settings| settings.max_iterations)
        .unwrap_or(MAX_TASK_ITERATIONS as usize);
    if !(2..=10_000).contains(&configured) {
        return Err("账号的本地任务最大迭代次数超出允许范围。".into());
    }
    u32::try_from(configured).map_err(|_| "账号的本地任务最大迭代次数无效。".into())
}

pub(super) async fn publish_capabilities(
    host: &mut ManagedHost,
    owner_user_id: &str,
    gateway_revision: &str,
    task_enabled_refs: &[String],
    task_max_iterations: u32,
) -> Result<(), String> {
    let revision = capability_revision(gateway_revision);
    let plugin_installer = LocalCapabilityPolicySnapshot {
        owner_user_id: owner_user_id.to_owned(),
        profile_key: MAIN_CHAT_PROFILE.into(),
        capability_policy_revision: plugin_installer_revision(&revision),
        instructions: Some(PLUGIN_INSTALLER_INSTRUCTIONS.into()),
        prefixed_input_items: Vec::new(),
        tools: plugin_installer_tool_schemas(),
    };
    let task_policy = json!({
        "max_iterations": task_max_iterations,
        "allowed_model_config_ids": task_enabled_refs,
        "enabled_builtin_kinds": ["CodeMaintainerRead", "CodeMaintainerWrite", "TerminalController"],
        "external_mcp_config_ids": [],
        "plugin_components": [],
        "plugin_release_snapshots": []
    })
    .to_string();
    for snapshot in [
        // Publish this before the normal main-chat snapshot so GetLatest remains
        // the regular capability while the installer is selected by exact revision.
        plugin_installer,
        LocalCapabilityPolicySnapshot {
            owner_user_id: owner_user_id.to_owned(),
            profile_key: MAIN_CHAT_PROFILE.into(),
            capability_policy_revision: revision.clone(),
            instructions: Some(strip_skill_frontmatter(MAIN_CHAT_INSTRUCTIONS)),
            prefixed_input_items: Vec::new(),
            tools: {
                let mut tools = vec![attachment_read_tool_schema()];
                tools.extend(team_asset_tool_schemas());
                tools.extend(project_dashboard_tool_schemas());
                tools.extend(team_chat_read_tool_schemas());
                tools.extend(team_chat_write_tool_schemas());
                tools.extend(todo_planning_read_tool_schemas());
                tools.extend(todo_planning_write_tool_schemas());
                tools
            },
        },
        LocalCapabilityPolicySnapshot {
            owner_user_id: owner_user_id.to_owned(),
            profile_key: TASK_EXECUTION_PROFILE.into(),
            capability_policy_revision: revision.clone(),
            instructions: Some(strip_skill_frontmatter(TASK_EXECUTION_INSTRUCTIONS)),
            prefixed_input_items: Vec::new(),
            tools: {
                let mut tools = vec![attachment_read_tool_schema()];
                tools.extend(team_asset_tool_schemas());
                tools.extend(project_read_tool_schemas());
                tools.extend(project_write_tool_schemas());
                tools.extend(terminal_tool_schemas());
                tools.extend(todo_execution_tool_schemas());
                tools.extend(capability_broker_tool_schemas());
                tools
            },
        },
        LocalCapabilityPolicySnapshot {
            owner_user_id: owner_user_id.to_owned(),
            profile_key: TASK_POLICY_PROFILE.into(),
            capability_policy_revision: revision.clone(),
            instructions: Some(task_policy),
            prefixed_input_items: Vec::new(),
            tools: Vec::new(),
        },
    ] {
        let result = request(
            host,
            HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                snapshot,
            }),
        )
        .await
        .map_err(|error| error.message)?;
        if !matches!(result, HostResult::CapabilityPolicySnapshot { .. }) {
            return Err("Local Agent Host 返回了错误的能力发布响应。".into());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn model_revision(
    id: &str,
    provider: &str,
    model: &str,
    base_url: &str,
    thinking_level: Option<&str>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    model_capability_revision: &str,
    context_window_tokens: u64,
    output_reserve_tokens: u64,
    protocol_overhead_estimator_id: &str,
    tokenizer_id: Option<&str>,
    supports_images: bool,
    supports_responses: bool,
) -> String {
    let fields = [
        id.to_owned(),
        provider.to_owned(),
        model.to_owned(),
        base_url.to_owned(),
        thinking_level.unwrap_or_default().to_owned(),
        temperature
            .map(|value| value.to_string())
            .unwrap_or_default(),
        max_output_tokens
            .map(|value| value.to_string())
            .unwrap_or_default(),
        model_capability_revision.to_owned(),
        context_window_tokens.to_string(),
        output_reserve_tokens.to_string(),
        protocol_overhead_estimator_id.to_owned(),
        tokenizer_id.unwrap_or_default().to_owned(),
        supports_images.to_string(),
        supports_responses.to_string(),
    ];
    let mut digest = Sha256::new();
    digest.update(fields.join("\0").as_bytes());
    format!("sha256-{:x}", digest.finalize())
}

pub(super) fn capability_revision(gateway_revision: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(gateway_revision.as_bytes());
    digest.update([0]);
    digest.update(WINDOWS_CAPABILITY_SCHEMA.as_bytes());
    format!("windows-local-agent-{:x}", digest.finalize())
}

pub(super) fn strip_skill_frontmatter(source: &str) -> String {
    let trimmed = source.trim();
    let Some(rest) = trimmed.strip_prefix("---\n") else {
        return trimmed.to_owned();
    };
    rest.split_once("\n---\n")
        .map(|(_, body)| body.trim().to_owned())
        .unwrap_or_else(|| trimmed.to_owned())
}
