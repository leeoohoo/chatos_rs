use super::authorization::{optional_string, required_string};
use crate::plugin_installer::{
    NpmMcpDraft, NpmMcpInspection, ProgramMcpDraft, ProgramMcpInspection,
};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub(super) fn npm_mcp_draft(arguments: &Map<String, Value>) -> Result<NpmMcpDraft, String> {
    let package_name = required_string(arguments, "package_name", 214)?;
    let binary_name = required_string(arguments, "binary_name", 128)?;
    Ok(NpmMcpDraft {
        display_name: optional_string(arguments, "display_name", 120)?
            .unwrap_or_else(|| package_name.clone()),
        description: optional_string(arguments, "description", 2_000)?.unwrap_or_default(),
        component_key: optional_string(arguments, "component_key", 128)?
            .unwrap_or_else(|| binary_name.clone()),
        package_name,
        package_version: required_string(arguments, "package_version", 128)?,
        binary_name,
        arguments: required_string_array(arguments, "arguments", 64, 2_048)?,
        access_profile: required_string(arguments, "access_profile", 32)?,
    })
}

pub(super) fn npm_inspection_output(inspection: &NpmMcpInspection, draft: &NpmMcpDraft) -> Value {
    let tool_names = inspection
        .tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    json!({
        "status": "inspection_passed",
        "fingerprint": inspection.fingerprint,
        "display_name": inspection.display_name,
        "version": inspection.version,
        "source_summary": inspection.source_summary,
        "components": [{
            "component_key": inspection.component_key,
            "display_name": inspection.display_name,
            "kind": "mcp",
            "tool_names": tool_names
        }],
        "resolved_install_request": {
            "display_name": draft.display_name,
            "description": draft.description,
            "component_key": draft.component_key,
            "package_name": draft.package_name,
            "package_version": draft.package_version,
            "binary_name": draft.binary_name,
            "arguments": draft.arguments,
            "access_profile": draft.access_profile
        }
    })
}

pub(super) fn program_inspection_output(
    inspection: &ProgramMcpInspection,
    source: &PluginSourceArguments,
) -> Value {
    let tool_names = inspection
        .tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    json!({
        "status": "inspection_passed",
        "fingerprint": inspection.fingerprint,
        "source_kind": source.kind(),
        "resolved_source": source.resolved_source(),
        "display_name": inspection.display_name,
        "version": inspection.version,
        "source_summary": inspection.source_summary,
        "components": [{
            "component_key": inspection.component_key,
            "kind": "program_mcp",
            "tool_names": tool_names,
        }],
    })
}

pub(super) fn validate_npm_argument_keys(
    arguments: &Map<String, Value>,
    install: bool,
) -> Result<(), String> {
    let allowed = [
        "display_name",
        "description",
        "component_key",
        "package_name",
        "package_version",
        "binary_name",
        "arguments",
        "access_profile",
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

fn required_string_array(
    arguments: &Map<String, Value>,
    field: &str,
    maximum_items: usize,
    maximum_length: usize,
) -> Result<Vec<String>, String> {
    let values = arguments
        .get(field)
        .and_then(Value::as_array)
        .filter(|values| values.len() <= maximum_items)
        .ok_or_else(|| format!("Plugin 安装字段无效：{field}"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| value.len() <= maximum_length && !value.contains('\0'))
                .map(str::to_owned)
                .ok_or_else(|| format!("Plugin 安装字段无效：{field}"))
        })
        .collect()
}

#[derive(Clone)]
pub(super) enum PluginSourceArguments {
    Local {
        location: PathBuf,
        subdirectory: Option<String>,
    },
    Git {
        location: String,
        git_reference: Option<String>,
        subdirectory: Option<String>,
    },
    Npm {
        package_name: String,
        exact_version: String,
        subdirectory: Option<String>,
    },
    Program {
        draft: ProgramMcpDraft,
    },
}

impl PluginSourceArguments {
    pub(super) fn kind(&self) -> &str {
        match self {
            Self::Local { .. } => "local_folder",
            Self::Git { .. } => "git_repository",
            Self::Npm { .. } => "npm_package",
            Self::Program { draft } => draft.kind.as_str(),
        }
    }

    pub(super) fn resolved_source(&self) -> Value {
        match self {
            Self::Local {
                location,
                subdirectory,
            } => json!({
                "source_kind": "local_folder",
                "location": location.to_string_lossy(),
                "subdirectory": subdirectory
            }),
            Self::Npm {
                package_name,
                exact_version,
                subdirectory,
            } => json!({
                "source_kind": "npm_package",
                "location": package_name,
                "exact_version": exact_version,
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
            Self::Program { draft } => json!({
                "source_kind": draft.kind,
                "location": draft.location,
                "exact_version": draft.exact_version,
                "program": {
                    "display_name": draft.display_name,
                    "component_key": draft.component_key,
                    "executable_path": draft.executable_path,
                    "arguments": draft.arguments,
                    "access_profile": draft.access_profile,
                    "asset_name": draft.asset_name,
                    "archive_sha256": draft.archive_sha256,
                }
            }),
        }
    }
}

pub(super) fn source_arguments(
    arguments: &Map<String, Value>,
    authorized_message: &str,
) -> Result<PluginSourceArguments, String> {
    let source_kind = required_string(arguments, "source_kind", 32)?;
    let location = required_string(arguments, "location", 4_096)?;
    let subdirectory = optional_string(arguments, "subdirectory", 1_024)?;
    let git_reference = optional_string(arguments, "git_reference", 256)?;
    let exact_version = optional_string(arguments, "exact_version", 128)?;
    let program = arguments
        .get("program")
        .map(|value| program_draft(value, &source_kind, &location, exact_version.as_deref()))
        .transpose()?;
    let message = authorized_message.to_lowercase();
    match source_kind.as_str() {
        "local_folder" => {
            let path = Path::new(&location);
            if !path.is_absolute()
                || git_reference.is_some()
                || exact_version.is_some()
                || program.is_some()
                || !message.contains(&location.to_lowercase())
            {
                return Err(
                    "本地 Plugin 必须使用用户明确给出的绝对目录，且不能提供版本或 Git ref。".into(),
                );
            }
            Ok(PluginSourceArguments::Local {
                location: path.to_path_buf(),
                subdirectory,
            })
        }
        "git_repository" => {
            if exact_version.is_some()
                || program.is_some()
                || !message.contains(&location.to_lowercase())
            {
                return Err(
                    "Git Plugin 必须使用用户明确给出的仓库地址，且不能提供 npm 版本。".into(),
                );
            }
            if git_reference
                .as_ref()
                .is_some_and(|reference| !message.contains(&reference.to_lowercase()))
            {
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
        "npm_package" => {
            if git_reference.is_some() {
                return Err("npm Plugin 不能提供 Git ref。".into());
            }
            if program.is_some() {
                return Err("npm Plugin 不能提供 program 启动配置。".into());
            }
            let exact_version = exact_version
                .ok_or_else(|| "npm Plugin 必须使用 resolve 返回的精确版本。".to_owned())?;
            Ok(PluginSourceArguments::Npm {
                package_name: location,
                exact_version,
                subdirectory,
            })
        }
        "github_release" | "local_executable" => {
            if git_reference.is_some() || subdirectory.is_some() || program.is_none() {
                return Err(
                    "程序 MCP 必须提供 program，且不能提供 Git ref 或 subdirectory。".into(),
                );
            }
            let exact_version = exact_version
                .ok_or_else(|| "程序 MCP 必须使用固定版本或 Release Tag。".to_owned())?;
            if !message.contains(&location.to_lowercase())
                || !message.contains(&exact_version.to_lowercase())
            {
                return Err("程序 MCP 来源和版本必须由用户在当前消息中明确给出。".into());
            }
            let Some(mut draft) = program else {
                unreachable!();
            };
            draft.exact_version = exact_version;
            Ok(PluginSourceArguments::Program { draft })
        }
        _ => {
            Err("Windows 当前只准入本机目录、HTTPS Git、固定版本 npm 或受信程序 MCP 来源。".into())
        }
    }
}

fn program_draft(
    value: &Value,
    kind: &str,
    location: &str,
    exact_version: Option<&str>,
) -> Result<ProgramMcpDraft, String> {
    let fields = value
        .as_object()
        .ok_or_else(|| "program 参数必须是对象。".to_owned())?;
    let allowed = [
        "display_name",
        "component_key",
        "executable_path",
        "arguments",
        "access_profile",
        "asset_name",
        "archive_sha256",
    ];
    if fields.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("program 参数包含未准入字段。".into());
    }
    Ok(ProgramMcpDraft {
        kind: kind.to_owned(),
        location: location.to_owned(),
        exact_version: exact_version.unwrap_or_default().to_owned(),
        display_name: required_string(fields, "display_name", 120)?,
        component_key: required_string(fields, "component_key", 128)?,
        executable_path: required_string(fields, "executable_path", 1_024)?,
        arguments: required_string_array(fields, "arguments", 64, 2_048)?,
        access_profile: required_string(fields, "access_profile", 32)?,
        asset_name: optional_string(fields, "asset_name", 240)?,
        archive_sha256: optional_string(fields, "archive_sha256", 64)?,
    })
}

pub(super) fn validate_source_argument_keys(
    arguments: &Map<String, Value>,
    install: bool,
) -> Result<(), String> {
    let allowed = [
        "source_kind",
        "location",
        "exact_version",
        "git_reference",
        "subdirectory",
        "program",
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

pub(super) fn validate_keys(
    arguments: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), String> {
    if arguments.keys().any(|key| !allowed.contains(&key.as_str())) {
        Err("Plugin 安装工具包含未准入字段。".into())
    } else {
        Ok(())
    }
}
