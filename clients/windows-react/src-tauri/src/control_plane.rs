use crate::authentication::{
    model_credential_environment_variable, AuthenticationManager, GatewayAgentCapability,
    GatewayModelConfig,
};
use crate::host_lifecycle::transport::{request, ManagedHost};
use crate::{
    attachment_tool_worker::{
        attachment_read_tool_schema, team_asset_tool_schemas, team_chat_read_tool_schemas,
        team_chat_write_tool_schemas, todo_execution_tool_schemas, todo_planning_read_tool_schemas,
        todo_planning_write_tool_schemas,
    },
    terminal_tool_worker::{
        project_read_tool_schemas, project_write_tool_schemas, terminal_tool_schemas,
    },
};
use chatos_local_agent_protocol::{
    GetCapabilityPolicySnapshotCommand, GetLatestCapabilityPolicySnapshotCommand, HostCommand,
    HostResult, ListLatestModelConfigSnapshotsCommand, LocalCapabilityPolicySnapshot,
    LocalModelConfigSnapshot, PutCapabilityPolicySnapshotCommand, PutModelConfigSnapshotCommand,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::OnceLock,
};

const MAIN_CHAT_PROFILE: &str = "main_chat";
const TASK_EXECUTION_PROFILE: &str = "task_execution";
const TASK_POLICY_PROFILE: &str = "task_policy_internal";
const EXECUTION_AGENT_KEY: &str = "local_agent_execution_agent";
const WINDOWS_CAPABILITY_SCHEMA: &str = "windows-host-team-chat-v12";
const MAX_TASK_ITERATIONS: u32 = 600;

const MAIN_CHAT_INSTRUCTIONS: &str = include_str!(
    "../../../macos/Sources/ChatOSCore/AgentSkills/Skills/chatos-main-chat-invariants/SKILL.md"
);
const TASK_EXECUTION_INSTRUCTIONS: &str = include_str!(
    "../../../macos/Sources/ChatOSCore/AgentSkills/Skills/chatos-task-execution-invariants/SKILL.md"
);
const MODEL_REASONING_CAPABILITIES: &str = include_str!(
    "../../../macos/Sources/ChatOSModelCapabilities/Resources/ModelReasoningCapabilities.json"
);

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ControlPlaneModelOption {
    pub id: String,
    pub display_name: String,
    pub model_name: String,
    pub provider: String,
    pub default_thinking_level: String,
    pub supports_reasoning: bool,
    pub thinking_levels: Vec<String>,
    #[serde(skip)]
    pub model_config_revision: String,
}

pub(crate) struct PreparedControlPlane {
    pub credential_environment: BTreeMap<String, String>,
    pub model_options: Vec<ControlPlaneModelOption>,
    pub restored_offline: bool,
}

struct PersistedControlPlane {
    snapshots: Vec<LocalModelConfigSnapshot>,
    credential_environment: BTreeMap<String, String>,
    complete_capabilities: bool,
}

struct RefreshedControlPlane {
    credential_environment: BTreeMap<String, String>,
    model_options: Vec<ControlPlaneModelOption>,
}

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

pub(crate) async fn synchronize(
    authentication: &AuthenticationManager,
    host: &mut ManagedHost,
    owner_user_id: &str,
) -> Result<PreparedControlPlane, String> {
    let persisted = restore_persisted(authentication, host, owner_user_id).await?;
    match refresh_from_gateway(authentication, host, owner_user_id, &persisted).await {
        Ok(refreshed) => Ok(PreparedControlPlane {
            credential_environment: refreshed.credential_environment,
            model_options: refreshed.model_options,
            restored_offline: false,
        }),
        Err(_network_error)
            if !persisted.snapshots.is_empty()
                && !persisted.credential_environment.is_empty()
                && persisted.complete_capabilities =>
        {
            Ok(PreparedControlPlane {
                credential_environment: persisted.credential_environment,
                model_options: persisted
                    .snapshots
                    .iter()
                    .map(model_option_from_snapshot)
                    .collect(),
                restored_offline: true,
            })
        }
        Err(network_error) => Err(format!(
            "无法准备本地模型配置：{network_error}。本机也没有完整的离线配置。"
        )),
    }
}

async fn restore_persisted(
    authentication: &AuthenticationManager,
    host: &mut ManagedHost,
    owner_user_id: &str,
) -> Result<PersistedControlPlane, String> {
    let result = request(
        host,
        HostCommand::ListLatestModelConfigSnapshots(ListLatestModelConfigSnapshotsCommand {
            owner_user_id: owner_user_id.to_owned(),
        }),
    )
    .await
    .map_err(|error| error.message)?;
    let HostResult::ModelConfigSnapshots { snapshots } = result else {
        return Err("Local Agent Host 返回了错误的模型配置响应。".into());
    };
    let mut credential_environment = BTreeMap::new();
    let mut usable_snapshots = Vec::new();
    for snapshot in snapshots {
        if snapshot.owner_user_id != owner_user_id {
            continue;
        }
        let variable = model_credential_environment_variable(&snapshot.model_config_ref);
        if snapshot.credential_ref != format!("env:{variable}") {
            continue;
        }
        let Some(credential) = authentication
            .load_model_credential(owner_user_id, &snapshot.model_config_ref)?
            .and_then(non_empty)
        else {
            continue;
        };
        if credential_environment
            .insert(variable, credential)
            .is_some()
        {
            return Err("本地模型配置产生了重复的凭据环境变量。".into());
        }
        usable_snapshots.push(snapshot);
    }
    let complete_capabilities = persisted_capabilities_are_complete(host, owner_user_id).await;
    Ok(PersistedControlPlane {
        snapshots: usable_snapshots,
        credential_environment,
        complete_capabilities,
    })
}

async fn persisted_capabilities_are_complete(host: &mut ManagedHost, owner_user_id: &str) -> bool {
    let Ok(HostResult::CapabilityPolicySnapshot { snapshot: main }) = request(
        host,
        HostCommand::GetLatestCapabilityPolicySnapshot(GetLatestCapabilityPolicySnapshotCommand {
            owner_user_id: owner_user_id.to_owned(),
            profile_key: MAIN_CHAT_PROFILE.into(),
        }),
    )
    .await
    else {
        return false;
    };
    if !valid_main_chat_snapshot(&main, owner_user_id) {
        return false;
    }
    for profile_key in [TASK_EXECUTION_PROFILE, TASK_POLICY_PROFILE] {
        let result = request(
            host,
            HostCommand::GetCapabilityPolicySnapshot(GetCapabilityPolicySnapshotCommand {
                owner_user_id: owner_user_id.to_owned(),
                profile_key: profile_key.into(),
                capability_policy_revision: main.capability_policy_revision.clone(),
            }),
        )
        .await;
        let Ok(HostResult::CapabilityPolicySnapshot { snapshot }) = result else {
            return false;
        };
        let valid = match profile_key {
            TASK_EXECUTION_PROFILE => valid_task_execution_snapshot(&snapshot, owner_user_id),
            TASK_POLICY_PROFILE => valid_task_policy_snapshot(&snapshot, owner_user_id),
            _ => false,
        };
        if !valid {
            return false;
        }
    }
    true
}

fn valid_main_chat_snapshot(snapshot: &LocalCapabilityPolicySnapshot, owner_user_id: &str) -> bool {
    let mut expected_tools = vec![attachment_read_tool_schema()];
    expected_tools.extend(team_asset_tool_schemas());
    expected_tools.extend(team_chat_read_tool_schemas());
    expected_tools.extend(team_chat_write_tool_schemas());
    expected_tools.extend(todo_planning_read_tool_schemas());
    expected_tools.extend(todo_planning_write_tool_schemas());
    snapshot.owner_user_id == owner_user_id
        && snapshot.profile_key == MAIN_CHAT_PROFILE
        && snapshot.tools == expected_tools
}

fn valid_task_execution_snapshot(
    snapshot: &LocalCapabilityPolicySnapshot,
    owner_user_id: &str,
) -> bool {
    let mut expected_tools = vec![attachment_read_tool_schema()];
    expected_tools.extend(team_asset_tool_schemas());
    expected_tools.extend(project_read_tool_schemas());
    expected_tools.extend(project_write_tool_schemas());
    expected_tools.extend(terminal_tool_schemas());
    expected_tools.extend(todo_execution_tool_schemas());
    snapshot.owner_user_id == owner_user_id
        && snapshot.profile_key == TASK_EXECUTION_PROFILE
        && snapshot.tools == expected_tools
}

fn valid_task_policy_snapshot(
    snapshot: &LocalCapabilityPolicySnapshot,
    owner_user_id: &str,
) -> bool {
    if snapshot.owner_user_id != owner_user_id
        || snapshot.profile_key != TASK_POLICY_PROFILE
        || !snapshot.tools.is_empty()
    {
        return false;
    }
    snapshot
        .instructions
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .and_then(|value| {
            value
                .get("enabled_builtin_kinds")
                .and_then(serde_json::Value::as_array)
                .cloned()
        })
        .is_some_and(|values| values == vec![json!("TerminalController")])
}

async fn refresh_from_gateway(
    authentication: &AuthenticationManager,
    host: &mut ManagedHost,
    owner_user_id: &str,
    persisted: &PersistedControlPlane,
) -> Result<RefreshedControlPlane, String> {
    let catalog = authentication
        .model_catalog_for_owner(owner_user_id)
        .await
        .map_err(|error| error.message)?;
    let selectable = catalog
        .into_iter()
        .filter(|model| {
            model.owner_user_id == owner_user_id
                && model.enabled
                && model.has_api_key
                && !model.model.trim().is_empty()
        })
        .collect::<Vec<_>>();
    if selectable.is_empty() {
        return Err("账号没有已启用且带凭据的模型。".into());
    }
    let model_config_refs = selectable
        .iter()
        .map(|model| model.id.clone())
        .collect::<Vec<_>>();
    let (resolved, capability) = tokio::join!(
        authentication.resolved_models_for_owner(owner_user_id, model_config_refs),
        authentication.local_agent_capability_for_owner(owner_user_id),
    );
    let resolved = resolved.map_err(|error| error.message)?;
    let capability = capability.map_err(|error| error.message)?;
    validate_capability(&capability, owner_user_id)?;

    let mut environment = persisted.credential_environment.clone();
    let mut refreshed_variables = BTreeSet::new();
    let mut snapshots = Vec::new();
    let mut model_options = Vec::new();
    let mut task_enabled_refs = Vec::new();
    for (item, resolved) in selectable.into_iter().zip(resolved) {
        if item.id != resolved.id {
            return Err("模型目录刷新顺序不一致。".into());
        }
        let mut option = model_option_from_gateway(&resolved);
        let (snapshot, credential, variable) = resolved_snapshot(resolved, owner_user_id)?;
        option.model_config_revision = snapshot.model_config_revision.clone();
        if !refreshed_variables.insert(variable.clone()) {
            return Err("模型标识产生了重复的凭据环境变量。".into());
        }
        authentication.save_model_credential(
            owner_user_id,
            &snapshot.model_config_ref,
            &credential,
        )?;
        environment.insert(variable, credential);
        if item.task_enabled.unwrap_or(item.enabled) {
            task_enabled_refs.push(snapshot.model_config_ref.clone());
        }
        publish_model(host, snapshot.clone()).await?;
        snapshots.push(snapshot);
        model_options.push(option);
    }
    if snapshots.is_empty() {
        return Err("账号没有可用于本地会话的模型。".into());
    }
    publish_capabilities(
        host,
        owner_user_id,
        &capability.policy_revision,
        &task_enabled_refs,
    )
    .await?;
    Ok(RefreshedControlPlane {
        credential_environment: environment,
        model_options,
    })
}

fn validate_capability(
    capability: &GatewayAgentCapability,
    owner_user_id: &str,
) -> Result<(), String> {
    if capability.owner_user_id != owner_user_id
        || capability.agent_key != EXECUTION_AGENT_KEY
        || !capability.agent_enabled
        || capability.policy_revision.trim().is_empty()
    {
        return Err("账号的 Local Agent 执行能力不可用。".into());
    }
    // Windows currently publishes only Host-owned tools. Server-side MCP/Plugin bindings are
    // intentionally not projected until a Windows platform tool worker exists.
    let _managed_binding_summary = capability
        .mcps
        .iter()
        .chain(capability.plugins.iter())
        .filter(|item| item.binding.enabled || item.binding.required)
        .map(|item| (item.available, item.status.as_str()))
        .collect::<Vec<_>>();
    Ok(())
}

fn resolved_snapshot(
    model: GatewayModelConfig,
    owner_user_id: &str,
) -> Result<(LocalModelConfigSnapshot, String, String), String> {
    if model.owner_user_id != owner_user_id || !model.enabled {
        return Err("模型配置与当前账号不匹配。".into());
    }
    let credential = model
        .api_key
        .and_then(non_empty)
        .ok_or_else(|| "模型配置没有可用凭据。".to_owned())?;
    let base_url = model
        .base_url
        .and_then(non_empty)
        .ok_or_else(|| "模型配置没有 API 地址。".to_owned())?;
    let parsed = reqwest::Url::parse(&base_url).map_err(|_| "模型 API 地址无效。")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("模型 API 地址无效。".into());
    }
    let model_name = required("model", model.model)?;
    let provider = required("provider", model.provider)?;
    let model_config_ref = required("model_config_ref", model.id)?;
    let variable = model_credential_environment_variable(&model_config_ref);
    let revision = model_revision(
        &model_config_ref,
        &provider,
        &model_name,
        &base_url,
        model.task_thinking_level.as_deref(),
        model.temperature,
        model.max_output_tokens,
        model.supports_images,
        model.supports_responses,
    );
    Ok((
        LocalModelConfigSnapshot {
            owner_user_id: owner_user_id.to_owned(),
            model_config_ref,
            model_config_revision: revision,
            credential_ref: format!("env:{variable}"),
            base_url,
            model: model_name,
            provider,
            supports_responses: model.supports_responses,
            supports_images: Some(model.supports_images),
            instructions: None,
            temperature: model.temperature,
            max_output_tokens: model.max_output_tokens,
            thinking_level: model.task_thinking_level.and_then(non_empty),
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            max_transient_retries: None,
            output_format: None,
        },
        credential,
        variable,
    ))
}

fn model_option_from_gateway(model: &GatewayModelConfig) -> ControlPlaneModelOption {
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

fn model_option_from_snapshot(snapshot: &LocalModelConfigSnapshot) -> ControlPlaneModelOption {
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

fn model_option(
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

async fn publish_model(
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

async fn publish_capabilities(
    host: &mut ManagedHost,
    owner_user_id: &str,
    gateway_revision: &str,
    task_enabled_refs: &[String],
) -> Result<(), String> {
    let revision = capability_revision(gateway_revision);
    let task_policy = json!({
        "max_iterations": MAX_TASK_ITERATIONS,
        "allowed_model_config_ids": task_enabled_refs,
        "enabled_builtin_kinds": ["CodeMaintainerRead", "CodeMaintainerWrite", "TerminalController"],
        "external_mcp_config_ids": [],
        "plugin_components": [],
        "plugin_release_snapshots": []
    })
    .to_string();
    for snapshot in [
        LocalCapabilityPolicySnapshot {
            owner_user_id: owner_user_id.to_owned(),
            profile_key: MAIN_CHAT_PROFILE.into(),
            capability_policy_revision: revision.clone(),
            instructions: Some(strip_skill_frontmatter(MAIN_CHAT_INSTRUCTIONS)),
            prefixed_input_items: Vec::new(),
            tools: {
                let mut tools = vec![attachment_read_tool_schema()];
                tools.extend(team_asset_tool_schemas());
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
fn model_revision(
    id: &str,
    provider: &str,
    model: &str,
    base_url: &str,
    thinking_level: Option<&str>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
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
        supports_images.to_string(),
        supports_responses.to_string(),
    ];
    let mut digest = Sha256::new();
    digest.update(fields.join("\0").as_bytes());
    format!("sha256-{:x}", digest.finalize())
}

fn capability_revision(gateway_revision: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(gateway_revision.as_bytes());
    digest.update([0]);
    digest.update(WINDOWS_CAPABILITY_SCHEMA.as_bytes());
    format!("windows-local-agent-{:x}", digest.finalize())
}

fn strip_skill_frontmatter(source: &str) -> String {
    let trimmed = source.trim();
    let Some(rest) = trimmed.strip_prefix("---\n") else {
        return trimmed.to_owned();
    };
    rest.split_once("\n---\n")
        .map(|(_, body)| body.trim().to_owned())
        .unwrap_or_else(|| trimmed.to_owned())
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(format!("{field} 无效。"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_are_deterministic_and_schema_scoped() {
        let first = capability_revision("server-revision-1");
        let second = capability_revision("server-revision-1");
        assert_eq!(first, second);
        assert_ne!(first, capability_revision("server-revision-2"));
        assert!(first.starts_with("windows-local-agent-"));
    }

    #[test]
    fn embedded_policy_uses_the_macos_source_without_yaml_frontmatter() {
        let policy = strip_skill_frontmatter(MAIN_CHAT_INSTRUCTIONS);
        assert!(policy.starts_with("# Main Chat policy"));
        assert!(!policy.contains("name: chatos-main-chat-invariants"));
    }

    #[test]
    fn model_options_reuse_the_shared_macos_reasoning_catalog() {
        let option = model_option(
            "model-1".into(),
            "GPT 6.1".into(),
            "gpt-6.1-sol".into(),
            "openai".into(),
            Some("none".into()),
            false,
            "revision-1".into(),
        );
        assert!(option.supports_reasoning);
        assert_eq!(option.default_thinking_level, "low");
        assert_eq!(
            option.thinking_levels,
            ["low", "medium", "high", "xhigh", "max"]
        );
    }

    #[test]
    fn offline_restore_rejects_the_old_attachment_only_execution_snapshot() {
        let owner = "owner-1";
        let snapshot = LocalCapabilityPolicySnapshot {
            owner_user_id: owner.into(),
            profile_key: TASK_EXECUTION_PROFILE.into(),
            capability_policy_revision: "old-revision".into(),
            instructions: Some("instructions".into()),
            prefixed_input_items: Vec::new(),
            tools: vec![attachment_read_tool_schema()],
        };
        assert!(!valid_task_execution_snapshot(&snapshot, owner));
    }

    #[test]
    fn current_project_and_terminal_snapshots_pass_offline_completeness_checks() {
        let owner = "owner-1";
        let mut tools = vec![attachment_read_tool_schema()];
        tools.extend(team_asset_tool_schemas());
        tools.extend(project_read_tool_schemas());
        tools.extend(project_write_tool_schemas());
        tools.extend(terminal_tool_schemas());
        tools.extend(todo_execution_tool_schemas());
        let execution = LocalCapabilityPolicySnapshot {
            owner_user_id: owner.into(),
            profile_key: TASK_EXECUTION_PROFILE.into(),
            capability_policy_revision: "current-revision".into(),
            instructions: Some("instructions".into()),
            prefixed_input_items: Vec::new(),
            tools,
        };
        let policy = LocalCapabilityPolicySnapshot {
            owner_user_id: owner.into(),
            profile_key: TASK_POLICY_PROFILE.into(),
            capability_policy_revision: "current-revision".into(),
            instructions: Some(
                json!({"enabled_builtin_kinds": ["TerminalController"]}).to_string(),
            ),
            prefixed_input_items: Vec::new(),
            tools: Vec::new(),
        };
        assert!(valid_task_execution_snapshot(&execution, owner));
        assert!(valid_task_policy_snapshot(&policy, owner));
    }
}
