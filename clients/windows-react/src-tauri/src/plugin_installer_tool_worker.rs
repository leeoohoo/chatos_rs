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
    plugin_installer::{inspect_git_package, inspect_local_package},
    plugin_settings::{
        install_git_plugin_source, install_local_plugin_source, PluginSettingsManager,
    },
};

pub(crate) const PLUGIN_INSTALLER_CONTACT_ID: &str = "plugin-installer";
pub(crate) const PLUGIN_INSTALLER_INSPECT_SOURCE: &str = "plugin_installer_inspect_source";
pub(crate) const PLUGIN_INSTALLER_INSTALL_SOURCE: &str = "plugin_installer_install_source";
pub(crate) const PLUGIN_INSTALLER_TOOL_NAMES: [&str; 2] = [
    PLUGIN_INSTALLER_INSPECT_SOURCE,
    PLUGIN_INSTALLER_INSTALL_SOURCE,
];

pub(crate) const PLUGIN_INSTALLER_INSTRUCTIONS: &str = r#"你是 ChatOS 的插件安装助手，只负责检查和安装用户明确指定的 ChatOS Plugin。

当前 Windows 准入范围包含 source_kind=local_folder 和 source_kind=git_repository。本地目录必须是用户在当前消息中明确给出的绝对路径；Git 必须是用户明确给出的、不含凭据/查询参数/片段的 HTTPS URL，可带安全的 git_reference 和 monorepo subdirectory。来源必须包含有效的 chatos.plugin.json 和 Windows stdio MCP 可执行组件。Git 获取禁用交互、Hook、文件协议和子模块，不运行仓库脚本。不要声称支持 npm、Codex、Claude、skills-only 或需要 Secret 的包。

必须先调用 plugin_installer_inspect_source。只有用户当前消息明确要求安装、添加、配置使用或启用时，才可以把完全相同的 source_kind、location、git_reference、subdirectory 和检查得到的 fingerprint 传给 plugin_installer_install_source。检查不是安装；重新获取后的内容变化时必须拒绝并重新检查。"#;

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
        "source_kind": {"type": "string", "enum": ["local_folder", "git_repository"]},
        "location": {
            "type": "string", "minLength": 1, "maxLength": 4096,
            "description": "An absolute local folder or a credential-free HTTPS Git repository URL according to source_kind."
        },
        "git_reference": {
            "type": "string", "maxLength": 256,
            "description": "Optional branch, tag, or commit for git_repository; forbidden for local_folder."
        },
        "subdirectory": {"type": "string", "maxLength": 1024}
    });
    vec![
        tool_schema(
            PLUGIN_INSTALLER_INSPECT_SOURCE,
            "Acquire and inspect a user-specified local folder or HTTPS Git ChatOS Plugin and return its bounded content fingerprint. This does not install it or run repository scripts.",
            source_properties.clone(),
            &["source_kind", "location"],
        ),
        tool_schema(
            PLUGIN_INSTALLER_INSTALL_SOURCE,
            "Re-acquire and install the exact local or HTTPS Git ChatOS Plugin only after a matching inspection and an explicit install request in this dedicated conversation.",
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
    validate_argument_keys(
        arguments,
        claim.invocation.tool_name == PLUGIN_INSTALLER_INSTALL_SOURCE,
    )?;
    let source = source_arguments(arguments, &authorized_message)?;
    match claim.invocation.tool_name.as_str() {
        PLUGIN_INSTALLER_INSPECT_SOURCE => {
            let inspection = match &source {
                PluginSourceArguments::Local {
                    location,
                    subdirectory,
                } => tauri::async_runtime::spawn_blocking({
                    let location = location.clone();
                    let subdirectory = subdirectory.clone();
                    move || inspect_local_package(&location, subdirectory.as_deref())
                })
                .await
                .map_err(|_| "Plugin 检查任务异常退出。".to_owned())??,
                PluginSourceArguments::Git {
                    location,
                    git_reference,
                    subdirectory,
                } => {
                    let settings = app.state::<PluginSettingsManager>();
                    inspect_git_package(
                        settings.plugin_root(),
                        location,
                        git_reference.as_deref(),
                        subdirectory.as_deref(),
                    )
                    .await?
                }
            };
            let components = inspection
                .manifest
                .mcp_servers
                .iter()
                .map(|server| server.component_key().to_owned())
                .collect::<Vec<_>>();
            Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({
                    "status": "inspected",
                    "source_kind": source.kind(),
                    "resolved_source": source.resolved_source(),
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
            let result = match source {
                PluginSourceArguments::Local {
                    location,
                    subdirectory,
                } => {
                    install_local_plugin_source(
                        app.state::<AuthenticationManager>().inner(),
                        app.state::<HostManager>().inner(),
                        app.state::<PluginSettingsManager>().inner(),
                        app.state::<PluginApplicationManager>().inner(),
                        owner_user_id,
                        location.to_string_lossy().into_owned(),
                        subdirectory,
                        fingerprint,
                    )
                    .await?
                }
                PluginSourceArguments::Git {
                    location,
                    git_reference,
                    subdirectory,
                } => {
                    install_git_plugin_source(
                        app.state::<AuthenticationManager>().inner(),
                        app.state::<HostManager>().inner(),
                        app.state::<PluginSettingsManager>().inner(),
                        app.state::<PluginApplicationManager>().inner(),
                        owner_user_id,
                        location,
                        git_reference,
                        subdirectory,
                        fingerprint,
                    )
                    .await?
                }
            };
            Ok(LocalAgentToolOutcome::Succeeded {
                output: serde_json::to_value(result)
                    .map_err(|_| "Plugin 安装结果无效。".to_owned())?,
            })
        }
        _ => Err("Plugin 安装工具不可用。".into()),
    }
}

#[derive(Clone)]
enum PluginSourceArguments {
    Local {
        location: PathBuf,
        subdirectory: Option<String>,
    },
    Git {
        location: String,
        git_reference: Option<String>,
        subdirectory: Option<String>,
    },
}

impl PluginSourceArguments {
    fn kind(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local_folder",
            Self::Git { .. } => "git_repository",
        }
    }

    fn resolved_source(&self) -> Value {
        match self {
            Self::Local {
                location,
                subdirectory,
            } => json!({
                "source_kind": "local_folder",
                "location": location.to_string_lossy(),
                "subdirectory": subdirectory
            }),
            Self::Git {
                location,
                git_reference,
                subdirectory,
            } => json!({
                "source_kind": "git_repository",
                "location": location,
                "git_reference": git_reference,
                "subdirectory": subdirectory
            }),
        }
    }
}

fn source_arguments(
    arguments: &Map<String, Value>,
    authorized_message: &str,
) -> Result<PluginSourceArguments, String> {
    let source_kind = required_string(arguments, "source_kind", 32)?;
    let location = required_string(arguments, "location", 4_096)?;
    if !authorized_message
        .to_lowercase()
        .contains(&location.to_lowercase())
    {
        return Err("Plugin 来源必须由用户在当前消息中明确给出。".into());
    }
    let subdirectory = optional_string(arguments, "subdirectory", 1_024)?;
    let git_reference = optional_string(arguments, "git_reference", 256)?;
    match source_kind.as_str() {
        "local_folder" => {
            let path = Path::new(&location);
            if !path.is_absolute() || git_reference.is_some() {
                return Err("本地 Plugin 必须使用绝对目录且不能提供 Git ref。".into());
            }
            Ok(PluginSourceArguments::Local {
                location: path.to_path_buf(),
                subdirectory,
            })
        }
        "git_repository" => {
            if git_reference.as_ref().is_some_and(|reference| {
                !authorized_message
                    .to_lowercase()
                    .contains(&reference.to_lowercase())
            }) {
                return Err(
                    "Git ref 必须由用户在当前消息中明确给出；否则请省略并使用远端 HEAD。".into(),
                );
            }
            Ok(PluginSourceArguments::Git {
                location,
                git_reference,
                subdirectory,
            })
        }
        _ => Err("Windows 当前只准入本机目录或 HTTPS Git Plugin。".into()),
    }
}

fn validate_argument_keys(arguments: &Map<String, Value>, install: bool) -> Result<(), String> {
    let allowed = [
        "source_kind",
        "location",
        "git_reference",
        "subdirectory",
        "expected_fingerprint",
    ];
    if arguments
        .keys()
        .any(|key| !allowed.contains(&key.as_str()) || (!install && key == "expected_fingerprint"))
    {
        return Err("Plugin 安装工具包含未准入字段。".into());
    }
    Ok(())
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
        assert_eq!(
            schemas[0]["function"]["parameters"]["properties"]["source_kind"]["enum"],
            json!(["local_folder", "git_repository"])
        );
    }

    #[test]
    fn install_intent_rejects_negation() {
        assert!(has_explicit_install_intent("请安装 C:\\safe\\plugin"));
        assert!(!has_explicit_install_intent(
            "先不要安装，只检查 C:\\safe\\plugin"
        ));
    }

    #[test]
    fn source_arguments_bind_git_url_to_the_current_user_message() {
        let arguments = json!({
            "source_kind": "git_repository",
            "location": "https://example.com/team/plugin.git",
            "git_reference": "release/3.0.9",
            "subdirectory": "packages/windows"
        });
        let source = source_arguments(
            arguments.as_object().unwrap(),
            "请检查 https://example.com/team/plugin.git 的 release/3.0.9，插件在 packages/windows",
        )
        .unwrap();
        assert!(matches!(
            source,
            PluginSourceArguments::Git {
                git_reference: Some(reference),
                subdirectory: Some(subdirectory),
                ..
            } if reference == "release/3.0.9" && subdirectory == "packages/windows"
        ));
        assert!(source_arguments(
            arguments.as_object().unwrap(),
            "请检查 https://example.com/team/plugin.git"
        )
        .is_err());
        assert!(source_arguments(arguments.as_object().unwrap(), "请检查那个插件").is_err());
    }

    #[test]
    fn local_sources_reject_git_fields_and_unknown_arguments() {
        let root = std::env::current_dir().unwrap().join("plugin-source");
        let arguments = json!({
            "source_kind": "local_folder",
            "location": root.to_string_lossy(),
            "git_reference": "main"
        });
        assert!(source_arguments(
            arguments.as_object().unwrap(),
            &format!("请检查 {}", root.display())
        )
        .is_err());

        let arguments = json!({
            "source_kind": "git_repository",
            "location": "https://example.com/plugin.git",
            "exact_version": "1.0.0"
        });
        assert!(validate_argument_keys(arguments.as_object().unwrap(), false).is_err());
    }
}
