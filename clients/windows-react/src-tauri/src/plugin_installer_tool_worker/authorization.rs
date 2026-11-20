use super::{
    plugin_installer_tool_schemas, PLUGIN_INSTALLER_CONTACT_ID, PLUGIN_INSTALLER_INSTRUCTIONS,
};
use crate::host_lifecycle::HostManager;
use chatos_local_agent_protocol::{
    GetCapabilityPolicySnapshotCommand, GetConversationCommand, GetRunCommand, HostCommand,
    HostResult, LocalAgentToolClaim, LocalConversationResourceKind,
};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};

pub(super) async fn authorize(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<String, String> {
    let host = app.state::<HostManager>();
    let result = host
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("Plugin 安装 Run 上下文无效。".into());
    };
    let input = run
        .input
        .as_object()
        .ok_or_else(|| "Plugin 安装 Run 上下文无效。".to_owned())?;
    let conversation_id = input
        .get("conversation_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin 安装 Run 上下文无效。".to_owned())?;
    let message = input
        .get("message")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin 安装 Run 上下文无效。".to_owned())?;
    if run.owner_user_id != owner_user_id
        || run.profile_key != "main_chat"
        || run.owner_entity_type != "conversation_turn"
        || !run
            .capability_policy_revision
            .starts_with("plugin-installer-")
    {
        return Err("Plugin 安装 Run 未获授权。".into());
    }
    let result = host
        .execute(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::Conversation { conversation } = result else {
        return Err("Plugin 安装会话无效。".into());
    };
    if !conversation
        .conversation
        .resource
        .as_ref()
        .is_some_and(|resource| {
            resource.kind == LocalConversationResourceKind::Contact
                && resource.resource_id == PLUGIN_INSTALLER_CONTACT_ID
        })
    {
        return Err("Plugin 安装工具只能在安装助手会话使用。".into());
    }
    let result = host
        .execute(HostCommand::GetCapabilityPolicySnapshot(
            GetCapabilityPolicySnapshotCommand {
                owner_user_id: owner_user_id.to_owned(),
                profile_key: "main_chat".into(),
                capability_policy_revision: run.capability_policy_revision,
            },
        ))
        .await?;
    let HostResult::CapabilityPolicySnapshot { snapshot } = result else {
        return Err("Plugin 安装能力无效。".into());
    };
    if snapshot.owner_user_id != owner_user_id
        || snapshot.profile_key != "main_chat"
        || snapshot.instructions.as_deref() != Some(PLUGIN_INSTALLER_INSTRUCTIONS)
        || snapshot.tools != plugin_installer_tool_schemas()
    {
        return Err("Plugin 安装能力未通过校验。".into());
    }
    Ok(message.to_owned())
}

pub(super) fn has_explicit_install_intent(message: &str) -> bool {
    let value = message.to_lowercase();
    let denied = [
        "不要安装",
        "不用安装",
        "先不安装",
        "do not install",
        "don't install",
    ];
    !denied.iter().any(|pattern| value.contains(pattern))
        && [
            "安装",
            "添加",
            "接入",
            "配置使用",
            "启用",
            "更新",
            "升级",
            "重新安装",
            "重装",
            "install",
            "add",
            "enable",
            "update",
            "upgrade",
            "reinstall",
        ]
        .iter()
        .any(|pattern| value.contains(pattern))
}

pub(super) fn has_update_intent(message: &str) -> bool {
    let value = message.to_lowercase();
    [
        "更新",
        "升级",
        "重新安装",
        "重装",
        "update",
        "upgrade",
        "reinstall",
    ]
    .iter()
    .any(|pattern| value.contains(pattern))
}

pub(super) fn matches_query(query: Option<&str>, fields: &[&str]) -> bool {
    let Some(query) = query else {
        return true;
    };
    let terms = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    terms.iter().all(|term| {
        fields
            .iter()
            .any(|field| field.to_lowercase().contains(term))
    })
}

pub(super) fn bounded_text(value: &str, maximum: usize, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return fallback.to_owned();
    }
    value.chars().take(maximum).collect()
}

pub(super) fn required_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<String, String> {
    let value = arguments
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if value.is_empty() || value.len() > maximum || value.contains('\0') {
        Err(format!("Plugin 安装字段无效：{field}"))
    } else {
        Ok(value.to_owned())
    }
}

pub(super) fn optional_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<Option<String>, String> {
    match arguments.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.len() <= maximum && !value.contains('\0') => {
            let value = value.trim();
            Ok((!value.is_empty()).then(|| value.to_owned()))
        }
        _ => Err(format!("Plugin 安装字段无效：{field}")),
    }
}
