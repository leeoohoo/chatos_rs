use chatos_local_agent_protocol::{
    GetCapabilityPolicySnapshotCommand, GetConversationCommand, GetRunCommand, HostCommand,
    HostResult, LocalAgentToolClaim, LocalAgentToolOutcome, LocalConversationResourceKind,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

use crate::{
    authentication::AuthenticationManager,
    host_lifecycle::HostManager,
    plugin_application::PluginApplicationManager,
    plugin_installer::inspect_local_package,
    plugin_settings::{install_local_plugin_source, PluginSettingsManager},
};

pub(crate) const PLUGIN_INSTALLER_CONTACT_ID: &str = "plugin-installer";
pub(crate) const PLUGIN_INSTALLER_INSPECT_SOURCE: &str = "plugin_installer_inspect_source";
pub(crate) const PLUGIN_INSTALLER_INSTALL_SOURCE: &str = "plugin_installer_install_source";
pub(crate) const PLUGIN_INSTALLER_TOOL_NAMES: [&str; 2] = [
    PLUGIN_INSTALLER_INSPECT_SOURCE,
    PLUGIN_INSTALLER_INSTALL_SOURCE,
];

pub(crate) const PLUGIN_INSTALLER_INSTRUCTIONS: &str = r#"你是 ChatOS 的插件安装助手，只负责检查和安装用户明确指定的本机 ChatOS Plugin 目录。

当前 Windows 准入范围只包含 source_kind=local_folder，目录必须是用户在当前消息中明确给出的绝对路径，并且必须包含有效的 chatos.plugin.json 和 Windows stdio MCP 可执行组件。不要声称支持 Git、npm、Codex、Claude、skills-only 或需要 Secret 的包。

必须先调用 plugin_installer_inspect_source。只有用户当前消息明确要求安装、添加、配置使用或启用时，才可以把完全相同的 location、subdirectory 和检查得到的 fingerprint 传给 plugin_installer_install_source。检查不是安装；目录内容变化后必须重新检查。"#;

pub(crate) fn plugin_installer_revision(main_revision: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(main_revision.as_bytes());
    digest.update([0]);
    digest.update(PLUGIN_INSTALLER_INSTRUCTIONS.as_bytes());
    digest.update([0]);
    digest.update(serde_json::to_vec(&plugin_installer_tool_schemas()).unwrap_or_default());
    format!("plugin-installer-{:x}", digest.finalize())
}

pub(crate) fn plugin_installer_tool_schemas() -> Vec<Value> {
    let source_properties = json!({
        "source_kind": {"type": "string", "enum": ["local_folder"]},
        "location": {"type": "string", "minLength": 1, "maxLength": 4096},
        "subdirectory": {"type": "string", "maxLength": 1024}
    });
    vec![
        tool_schema(
            PLUGIN_INSTALLER_INSPECT_SOURCE,
            "Inspect a user-specified absolute local ChatOS Plugin folder and return its bounded content fingerprint. This does not install it.",
            source_properties.clone(),
            &["source_kind", "location"],
        ),
        tool_schema(
            PLUGIN_INSTALLER_INSTALL_SOURCE,
            "Install the exact local ChatOS Plugin folder only after a matching inspection and an explicit install request in this dedicated conversation.",
            {
                let mut properties = source_properties.as_object().cloned().unwrap_or_default();
                properties.insert("expected_fingerprint".into(), json!({"type": "string", "pattern": "^[a-f0-9]{64}$"}));
                Value::Object(properties)
            },
            &["source_kind", "location", "expected_fingerprint"],
        ),
    ]
}

fn tool_schema(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            }
        }
    })
}

pub(crate) async fn execute_plugin_installer_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    let authorized_message = authorize(app, owner_user_id, claim).await?;
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "Plugin 安装工具参数无效。".to_owned())?;
    let source = source_arguments(arguments, &authorized_message)?;
    match claim.invocation.tool_name.as_str() {
        PLUGIN_INSTALLER_INSPECT_SOURCE => {
            let inspection = tauri::async_runtime::spawn_blocking({
                let source = source.clone();
                move || inspect_local_package(&source.location, source.subdirectory.as_deref())
            })
            .await
            .map_err(|_| "Plugin 检查任务异常退出。".to_owned())??;
            let components = inspection
                .manifest
                .mcp_servers
                .iter()
                .map(|server| server.component_key().to_owned())
                .collect::<Vec<_>>();
            Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({
                    "status": "inspected",
                    "source_kind": "local_folder",
                    "resolved_source": {
                        "source_kind": "local_folder",
                        "location": source.location.to_string_lossy(),
                        "subdirectory": source.subdirectory
                    },
                    "fingerprint": inspection.fingerprint,
                    "plugin": {
                        "name": inspection.manifest.name,
                        "display_name": inspection.manifest.interface.display_name,
                        "version": inspection.manifest.version,
                        "components": components
                    }
                }),
            })
        }
        PLUGIN_INSTALLER_INSTALL_SOURCE => {
            if !has_explicit_install_intent(&authorized_message) {
                return Err("当前用户消息没有明确授权安装 Plugin。".into());
            }
            let fingerprint = required_string(arguments, "expected_fingerprint", 64)?;
            let result = install_local_plugin_source(
                app.state::<AuthenticationManager>().inner(),
                app.state::<HostManager>().inner(),
                app.state::<PluginSettingsManager>().inner(),
                app.state::<PluginApplicationManager>().inner(),
                owner_user_id,
                source.location.to_string_lossy().into_owned(),
                source.subdirectory,
                fingerprint,
            )
            .await?;
            Ok(LocalAgentToolOutcome::Succeeded {
                output: serde_json::to_value(result)
                    .map_err(|_| "Plugin 安装结果无效。".to_owned())?,
            })
        }
        _ => Err("Plugin 安装工具不可用。".into()),
    }
}

#[derive(Clone)]
struct LocalSourceArguments {
    location: PathBuf,
    subdirectory: Option<String>,
}

fn source_arguments(
    arguments: &Map<String, Value>,
    authorized_message: &str,
) -> Result<LocalSourceArguments, String> {
    if required_string(arguments, "source_kind", 32)? != "local_folder" {
        return Err("Windows 当前只准入本机目录 Plugin。".into());
    }
    let location = required_string(arguments, "location", 4_096)?;
    let path = Path::new(&location);
    if !path.is_absolute()
        || !authorized_message
            .to_lowercase()
            .contains(&location.to_lowercase())
    {
        return Err("Plugin 目录必须由用户在当前消息中明确给出。".into());
    }
    let subdirectory = optional_string(arguments, "subdirectory", 1_024)?;
    Ok(LocalSourceArguments {
        location: path.to_path_buf(),
        subdirectory,
    })
}

async fn authorize(
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

fn has_explicit_install_intent(message: &str) -> bool {
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
            "install",
            "add",
            "enable",
        ]
        .iter()
        .any(|pattern| value.contains(pattern))
}

fn required_string(
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

fn optional_string(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_and_revision_are_stable_and_source_only() {
        let schemas = plugin_installer_tool_schemas();
        assert_eq!(schemas.len(), 2);
        assert_eq!(
            schemas[0]["function"]["name"],
            PLUGIN_INSTALLER_INSPECT_SOURCE
        );
        assert_eq!(
            plugin_installer_revision("main-1"),
            plugin_installer_revision("main-1")
        );
        assert_ne!(
            plugin_installer_revision("main-1"),
            plugin_installer_revision("main-2")
        );
    }

    #[test]
    fn install_intent_rejects_negation() {
        assert!(has_explicit_install_intent("请安装 C:\\safe\\plugin"));
        assert!(!has_explicit_install_intent(
            "先不要安装，只检查 C:\\safe\\plugin"
        ));
    }
}
