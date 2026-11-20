use super::*;

#[derive(Clone)]
pub(super) struct CompatiblePluginIdentity {
    name: String,
    version: String,
    description: String,
    display_name: String,
    format_name: String,
    document: serde_json::Map<String, Value>,
}

pub(super) fn normalize_compatible_mcp_package(package_root: &Path) -> Result<(), String> {
    let native_manifest = package_root.join("chatos.plugin.json");
    if native_manifest.is_file() {
        let raw = read_bounded_text(&native_manifest, MAXIMUM_MANIFEST_BYTES)?;
        if let Ok(manifest) = parse_plugin_manifest(&raw) {
            if validate_plugin_manifest(&manifest).is_ok() {
                return Ok(());
            }
        }
    }
    let identity = compatible_plugin_identity(package_root)?;
    let skills = normalize_compatible_skills(package_root)?;
    let hooks = match identity.as_ref() {
        Some(identity) => normalize_compatible_hooks(package_root, &identity.document)?,
        None => Vec::new(),
    };
    let external_configuration = COMPATIBLE_MCP_CONFIGURATIONS
        .iter()
        .map(|name| (*name, package_root.join(name)))
        .find(|(_, path)| path.is_file());
    let (configuration_name, raw, document) = if let Some((name, path)) = external_configuration {
        let raw = read_bounded_text(&path, MAXIMUM_MANIFEST_BYTES)?;
        let document = serde_json::from_str::<Value>(&raw)
            .map_err(|_| install_error("MCP 配置不是有效 JSON"))?;
        (Some(name.to_owned()), Some(raw), Some(document))
    } else if identity
        .as_ref()
        .is_some_and(|identity| identity.document.contains_key("mcpServers"))
    {
        let identity = identity.as_ref().expect("identity checked above");
        (
            Some(identity.format_name.clone()),
            Some(
                serde_json::to_string(&identity.document)
                    .map_err(|_| install_error("Plugin 清单无法序列化"))?,
            ),
            Some(Value::Object(identity.document.clone())),
        )
    } else {
        (None, None, None)
    };
    let servers = match document.as_ref() {
        Some(document) => {
            let document = document
                .as_object()
                .ok_or_else(|| install_error("MCP 配置必须是 JSON 对象"))?;
            match ["mcpServers", "servers", "mcp"]
                .iter()
                .find_map(|key| document.get(*key))
                .and_then(Value::as_object)
            {
                Some(servers) if !servers.is_empty() && servers.len() <= 64 => Some(servers),
                None if configuration_name.as_deref() == Some("opencode.json")
                    && document.len() == 1
                    && document.get("$schema").and_then(Value::as_str).is_some() =>
                {
                    None
                }
                _ => {
                    return Err(install_error(&format!(
                        "{} 缺少有效 MCP Server 对象",
                        configuration_name.as_deref().unwrap_or("Plugin 清单")
                    )))
                }
            }
        }
        None => None,
    };
    if identity.is_none() && skills.is_empty() && servers.is_none() && hooks.is_empty() {
        return Ok(());
    }

    let mut normalized_servers = BTreeMap::new();
    if let Some(servers) = servers {
        let bin_root = package_root.join("bin");
        ensure_owned_directory(&bin_root)
            .map_err(|_| install_error("无法创建兼容 MCP 启动目录"))?;
        let mut frozen_node_prepared = false;
        for (server_name, server) in servers {
            let server = server
                .as_object()
                .ok_or_else(|| install_error("MCP Server 配置必须是对象"))?;
            let transport = server
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("stdio")
                .to_ascii_lowercase();
            if !matches!(transport.as_str(), "stdio" | "local") {
                return Err(install_error("兼容导入当前只接受 stdio MCP"));
            }
            if let Some(environment) = server.get("env").or_else(|| server.get("environment")) {
                if !environment.is_null()
                    && !environment
                        .as_object()
                        .is_some_and(serde_json::Map::is_empty)
                {
                    return Err(install_error(
                        "兼容 MCP 配置不能安全区分凭据模板与明文环境变量，已拒绝导入",
                    ));
                }
            }
            let component_key = compatibility_identifier(server_name, "MCP Server 名称")?;
            if normalized_servers.contains_key(&component_key) {
                return Err(install_error("MCP Server 名称规范化后重复"));
            }
            let command = compatible_command(server, server_name)?;
            let raw_command = command
                .first()
                .ok_or_else(|| install_error("MCP Server 缺少 command"))?;
            let mut arguments = command.iter().skip(1).cloned().collect::<Vec<_>>();
            arguments.extend(compatible_string_array(server.get("args"), "MCP args")?);
            if arguments.len() > 64 {
                return Err(install_error("MCP 启动参数数量超过限制"));
            }
            let bin = prepare_compatible_launcher(
                package_root,
                &bin_root,
                &component_key,
                raw_command,
                &mut frozen_node_prepared,
            )?;
            normalized_servers.insert(
                component_key,
                json!({
                    "type": "stdio",
                    "bin": bin,
                    "args": arguments,
                    "env": {}
                }),
            );
        }
    }

    let components = normalized_servers.keys().cloned().collect::<Vec<_>>();
    let primary_component = components
        .first()
        .cloned()
        .unwrap_or_else(|| "local-mcp".into());
    let identity_component = primary_component.chars().take(40).collect::<String>();
    let identity_seed = raw.unwrap_or_else(|| skills.join("\n"));
    let configuration_digest = format!("{:x}", Sha256::digest(identity_seed.as_bytes()));
    let fallback_identity = format!("mcp-{identity_component}-{}", &configuration_digest[..12]);
    let default_display_name = if normalized_servers.len() == 1 {
        normalized_servers
            .keys()
            .next()
            .cloned()
            .unwrap_or(primary_component)
    } else if normalized_servers.is_empty() {
        skills
            .first()
            .and_then(|path| path.rsplit('/').next())
            .unwrap_or("Imported Skill")
            .to_owned()
    } else {
        format!("Imported MCP ({})", normalized_servers.len())
    };
    let manifest_name = identity
        .as_ref()
        .map(|identity| identity.name.clone())
        .unwrap_or_else(|| {
            if normalized_servers.is_empty() {
                skills
                    .first()
                    .and_then(|path| path.rsplit('/').next())
                    .unwrap_or("local-skill")
                    .to_owned()
            } else {
                format!("compat-{fallback_identity}")
            }
        });
    let manifest_version = identity
        .as_ref()
        .map(|identity| identity.version.clone())
        .unwrap_or_else(|| "0.0.0-local".into());
    let description = identity
        .as_ref()
        .map(|identity| identity.description.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if normalized_servers.is_empty() {
                "Imported standalone Agent Skill".into()
            } else {
                format!(
                    "Imported stdio MCP configuration from {}",
                    configuration_name.as_deref().unwrap_or("compatible Plugin")
                )
            }
        });
    let display_name = identity
        .as_ref()
        .map(|identity| identity.display_name.clone())
        .unwrap_or(default_display_name)
        .chars()
        .take(120)
        .collect::<String>();
    let mut permissions = Vec::new();
    if !normalized_servers.is_empty() {
        permissions.push(json!({
            "permission": "process.spawn",
            "required": true,
            "reason": "启动用户确认导入的本机 MCP Server",
            "components": components
        }));
    }
    if !hooks.is_empty() {
        let hook_components = hooks
            .iter()
            .filter_map(|hook| hook.get("componentKey").and_then(Value::as_str))
            .collect::<Vec<_>>();
        permissions.extend([
            json!({
                "permission": "process.spawn",
                "required": true,
                "reason": "运行用户逐条确认的生命周期 Hook 命令",
                "components": hook_components
            }),
            json!({
                "permission": "workspace.read",
                "required": true,
                "reason": "向本机只读 Hook 提供当前授权工作区",
                "components": hook_components
            }),
        ]);
    }
    write_json_file(
        &package_root.join("chatos.plugin.json"),
        &json!({
            "schemaVersion": 3,
            "name": manifest_name,
            "version": manifest_version,
            "description": description,
            "author": {"name": "Local"},
            "interface": {
                "displayName": display_name,
                "shortDescription": description,
                "longDescription": description,
                "developerName": "Local",
                "category": "Developer Tools"
            },
            "skills": skills,
            "mcpServers": normalized_servers,
            "hooks": hooks,
            "permissions": permissions
        }),
    )
}

pub(super) fn normalize_compatible_hooks(
    package_root: &Path,
    document: &serde_json::Map<String, Value>,
) -> Result<Vec<Value>, String> {
    let Some(raw) = document.get("hooks") else {
        return Ok(Vec::new());
    };
    let paths = match raw {
        Value::String(path) => vec![path.as_str()],
        Value::Array(paths) if paths.iter().all(Value::is_string) => {
            paths.iter().filter_map(Value::as_str).collect()
        }
        _ => {
            return Err(install_error(
                "Codex Plugin hooks 必须是路径字符串或路径数组",
            ))
        }
    };
    if paths.is_empty() || paths.len() > MAXIMUM_HOOK_BUNDLES {
        return Err(install_error("Codex Hook Bundle 数量无效"));
    }
    let mut component_keys = BTreeSet::new();
    paths
        .into_iter()
        .enumerate()
        .map(|(index, raw_path)| {
            let relative = normalized_hook_relative_path(raw_path)?;
            secure_hook_source(package_root, &relative)?;
            let file_stem = relative
                .file_stem()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("lifecycle-hooks-{index}"));
            let component_key = compatibility_identifier(&file_stem, "Hook component key")?;
            if !component_keys.insert(component_key.clone()) {
                return Err(install_error("Hook component key 重复"));
            }
            Ok(json!({
                "componentKey": component_key,
                "source": format!("./{}", relative.to_string_lossy().replace('\\', "/"))
            }))
        })
        .collect()
}

pub(super) fn compatible_plugin_identity(
    package_root: &Path,
) -> Result<Option<CompatiblePluginIdentity>, String> {
    let mut diagnostics = Vec::new();
    for (relative, format_name) in COMPATIBLE_PLUGIN_MANIFESTS {
        let path = package_root.join(relative);
        if !path.is_file() {
            continue;
        }
        match parse_compatible_plugin_identity(&path, format_name) {
            Ok(identity) => return Ok(Some(identity)),
            Err(error) => diagnostics.push(format!("{format_name}：{error}")),
        }
    }
    if diagnostics.is_empty() {
        Ok(None)
    } else {
        Err(install_error(&format!(
            "没有找到有效的兼容 Plugin 清单：{}",
            diagnostics.join("；")
        )))
    }
}

pub(super) fn parse_compatible_plugin_identity(
    path: &Path,
    format_name: &str,
) -> Result<CompatiblePluginIdentity, String> {
    let raw = read_bounded_text(path, MAXIMUM_MANIFEST_BYTES)?;
    let document = serde_json::from_str::<Value>(&raw)
        .map_err(|_| install_error(&format!("{format_name} 清单不是有效 JSON")))?
        .as_object()
        .cloned()
        .ok_or_else(|| install_error(&format!("{format_name} 清单必须是 JSON 对象")))?;
    let raw_name = document
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| install_error(&format!("{format_name} 清单必须声明 name 和 version")))?;
    let raw_version = document
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| install_error(&format!("{format_name} 清单必须声明 name 和 version")))?;
    let name = compatibility_identifier(raw_name, "Plugin name")?;
    let version = raw_version.trim();
    if version.is_empty() || version.len() > 128 || version.chars().any(char::is_control) {
        return Err(install_error("Plugin version 无效"));
    }
    let description = document
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .chars()
        .take(2_000)
        .collect::<String>();
    let display_name = compatible_display_name(&document)
        .unwrap_or(raw_name)
        .trim()
        .chars()
        .take(120)
        .collect::<String>();
    if display_name.is_empty() || display_name.chars().any(char::is_control) {
        return Err(install_error("Plugin displayName 无效"));
    }
    Ok(CompatiblePluginIdentity {
        name,
        version: version.to_owned(),
        description,
        display_name,
        format_name: format_name.to_owned(),
        document,
    })
}

pub(super) fn compatible_display_name(document: &serde_json::Map<String, Value>) -> Option<&str> {
    document
        .get("extensions")
        .and_then(Value::as_object)
        .and_then(|extensions| extensions.get("com.openai"))
        .and_then(Value::as_object)
        .and_then(|openai| openai.get("interface"))
        .and_then(Value::as_object)
        .and_then(|interface| interface.get("displayName"))
        .and_then(Value::as_str)
        .or_else(|| {
            document
                .get("interface")
                .and_then(Value::as_object)
                .and_then(|interface| interface.get("displayName"))
                .and_then(Value::as_str)
        })
        .or_else(|| document.get("displayName").and_then(Value::as_str))
}
