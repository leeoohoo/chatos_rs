use super::*;

pub(super) async fn prepare_npm_mcp_package(
    plugin_root: &Path,
    draft: &NpmMcpDraft,
) -> Result<AcquiredPluginSource, String> {
    validate_npm_mcp_draft(draft)?;
    let plugin_root = canonical_plugin_root(plugin_root)?;
    let staging_root = plugin_root
        .join(".source-staging")
        .join(Uuid::new_v4().to_string());
    let package_root = staging_root.join("package");
    ensure_owned_directory(&package_root)
        .map_err(|_| install_error("无法创建 npm MCP 临时目录"))?;
    let acquired = AcquiredPluginSource {
        plugin_root,
        staging_root,
        package_root,
    };

    let dependencies =
        BTreeMap::from([(draft.package_name.clone(), draft.package_version.clone())]);
    write_json_file(
        &acquired.package_root.join("package.json"),
        &json!({
            "name": "chatos-custom-mcp-host",
            "private": true,
            "version": "1.0.0",
            "dependencies": dependencies
        }),
    )?;
    let node = find_system_executable("node")
        .ok_or_else(|| install_error("npm MCP 需要本机 Node.js 运行时"))?;
    let npm_cli = resolve_npm_cli(&node)?;
    let npm_config_root = acquired.staging_root.join("npm-config");
    ensure_owned_directory(&npm_config_root)
        .map_err(|_| install_error("无法创建隔离 npm 配置目录"))?;
    let user_config = npm_config_root.join("user.npmrc");
    let global_config = npm_config_root.join("global.npmrc");
    create_private_file(&user_config).map_err(|_| install_error("无法创建隔离 npm 配置"))?;
    create_private_file(&global_config).map_err(|_| install_error("无法创建隔离 npm 配置"))?;
    run_isolated_npm(
        &node,
        &npm_cli,
        &acquired.package_root,
        &acquired.staging_root,
        &user_config,
        &global_config,
        &[
            "install",
            "--package-lock-only",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--registry=https://registry.npmjs.org/",
        ],
    )
    .await?;
    validate_npm_lock(&acquired.package_root.join("package-lock.json"))?;
    run_isolated_npm(
        &node,
        &npm_cli,
        &acquired.package_root,
        &acquired.staging_root,
        &user_config,
        &global_config,
        &[
            "ci",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--registry=https://registry.npmjs.org/",
        ],
    )
    .await?;
    sanitize_npm_tree(&acquired.package_root)?;

    let dependency_root = draft
        .package_name
        .split('/')
        .fold(acquired.package_root.join("node_modules"), |path, part| {
            path.join(part)
        });
    let dependency_manifest = read_bounded_text(
        &dependency_root.join("package.json"),
        MAXIMUM_PACKAGE_JSON_BYTES,
    )?;
    let dependency_manifest: Value = serde_json::from_str(&dependency_manifest)
        .map_err(|_| install_error("npm MCP package.json 无效"))?;
    if dependency_manifest.get("name").and_then(Value::as_str) != Some(draft.package_name.as_str())
        || dependency_manifest.get("version").and_then(Value::as_str)
            != Some(draft.package_version.as_str())
    {
        return Err(install_error("npm 安装结果与固定包名和版本不一致"));
    }
    let binary_path = npm_binary_path(
        dependency_manifest.get("bin"),
        &draft.package_name,
        &draft.binary_name,
    )?;
    let entry_relative = safe_relative_path(Path::new(&binary_path))?;
    let entry = dependency_root.join(&entry_relative);
    let entry_metadata =
        fs::symlink_metadata(&entry).map_err(|_| install_error("npm MCP 可执行入口不存在"))?;
    if !entry_metadata.is_file() || is_symlink_or_reparse(&entry_metadata) {
        return Err(install_error("npm MCP 可执行入口无效"));
    }
    let entry = entry
        .canonicalize()
        .map_err(|_| install_error("npm MCP 可执行入口无效"))?;
    let dependency_root = dependency_root
        .canonicalize()
        .map_err(|_| install_error("npm MCP 安装目录无效"))?;
    if !entry.starts_with(&dependency_root) {
        return Err(install_error("npm MCP 可执行入口越界"));
    }
    let canonical_package_root = acquired
        .package_root
        .canonicalize()
        .map_err(|_| install_error("npm MCP 安装目录无效"))?;
    let relative_entry = entry
        .strip_prefix(&canonical_package_root)
        .map_err(|_| install_error("npm MCP 可执行入口越界"))?
        .to_string_lossy()
        .replace('\\', "/");

    let bin_root = acquired.package_root.join("bin");
    ensure_owned_directory(&bin_root).map_err(|_| install_error("无法创建 npm MCP 启动目录"))?;
    let launcher = bin_root.join(format!("{}.exe", draft.component_key));
    fs::copy(&node, &launcher).map_err(|_| install_error("无法冻结 npm MCP Node.js 运行时"))?;
    let mut launch_arguments = vec![relative_entry];
    launch_arguments.extend(draft.arguments.iter().cloned());
    let permissions = npm_access_permissions(&draft.access_profile, &draft.component_key)?;
    let manifest_name = npm_mcp_manifest_name(&draft.package_name, &draft.component_key);
    let description = if draft.description.trim().is_empty() {
        format!("Custom npm MCP package {}", draft.package_name)
    } else {
        draft.description.clone()
    };
    let mcp_servers = BTreeMap::from([(
        draft.component_key.clone(),
        json!({
            "type": "stdio",
            "bin": draft.component_key,
            "args": launch_arguments,
            "env": {}
        }),
    )]);
    write_json_file(
        &acquired.package_root.join("chatos.plugin.json"),
        &json!({
            "schemaVersion": 3,
            "name": manifest_name,
            "version": draft.package_version,
            "description": description,
            "author": {"name": "npm"},
            "interface": {
                "displayName": draft.display_name,
                "shortDescription": description,
                "longDescription": description,
                "developerName": "npm",
                "category": "Developer Tools"
            },
            "mcpServers": mcp_servers,
            "permissions": permissions
        }),
    )?;
    validate_local_package(&acquired.package_root)?;
    Ok(acquired)
}

pub(super) async fn inspect_prepared_npm_mcp(
    package_root: &Path,
    draft: &NpmMcpDraft,
) -> Result<NpmMcpInspection, String> {
    let local = inspect_local_package(package_root, None)?;
    let server = local
        .manifest
        .mcp_servers
        .iter()
        .find(|server| server.component_key() == draft.component_key)
        .ok_or_else(|| install_error("npm MCP 组件清单无效"))?;
    let PluginMcpServer::Stdio { bin, args, env, .. } = server else {
        return Err(install_error("npm MCP 必须使用 stdio transport"));
    };
    let executable = resolve_executable(package_root, bin)?;
    let tools = inspect_mcp_tools(package_root, &executable, args, env).await?;
    let source_summary = format!(
        "npm {}@{} · {} {} · {}",
        draft.package_name,
        draft.package_version,
        draft.binary_name,
        draft.arguments.join(" "),
        draft.access_profile
    );
    let fingerprint = npm_mcp_fingerprint(&local.fingerprint, draft, &tools)?;
    Ok(NpmMcpInspection {
        fingerprint,
        display_name: draft.display_name.clone(),
        version: draft.package_version.clone(),
        source_summary,
        component_key: draft.component_key.clone(),
        tools,
        content_fingerprint: local.fingerprint,
    })
}

fn validate_npm_mcp_draft(draft: &NpmMcpDraft) -> Result<(), String> {
    validate_npm_package_name(&draft.package_name)?;
    validate_exact_version(&draft.package_version)?;
    validate_npm_binary_name(&draft.binary_name)?;
    if !valid_executable_name(&draft.component_key)
        || draft.display_name.trim().is_empty()
        || draft.display_name.len() > 120
        || draft.description.len() > 2_000
        || draft.display_name.contains('\0')
        || draft.description.contains('\0')
        || draft.arguments.len() > 64
        || draft
            .arguments
            .iter()
            .any(|value| value.len() > 2_048 || value.contains('\0'))
        || !matches!(
            draft.access_profile.as_str(),
            "restricted" | "network_client" | "developer_analysis"
        )
    {
        return Err(install_error("npm MCP 安装参数无效"));
    }
    Ok(())
}

fn npm_mcp_manifest_name(package_name: &str, component_key: &str) -> String {
    let mut stem = package_name
        .chars()
        .chain(['-'])
        .chain(component_key.chars())
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while stem.contains("--") {
        stem = stem.replace("--", "-");
    }
    stem = stem.trim_matches('-').chars().take(40).collect();
    if stem.is_empty() {
        stem = "custom-mcp".into();
    }
    let identity = format!("{package_name}\0{component_key}");
    let digest = format!("{:x}", Sha256::digest(identity.as_bytes()));
    format!("npm-{stem}-{}", &digest[..12])
}

pub(super) fn npm_binary_path(
    value: Option<&Value>,
    package_name: &str,
    binary_name: &str,
) -> Result<String, String> {
    let value = value.ok_or_else(|| install_error("npm package 没有声明 bin"))?;
    let path = if let Some(path) = value.as_str() {
        let implicit_name = package_name.rsplit('/').next().unwrap_or(package_name);
        (implicit_name == binary_name).then_some(path)
    } else {
        value
            .as_object()
            .and_then(|values| values.get(binary_name))
            .and_then(Value::as_str)
    }
    .ok_or_else(|| install_error("npm package 没有声明所选 bin"))?;
    bounded_npm_binary_path(path.trim_start_matches("./"))
}

pub(super) fn npm_access_permissions(
    profile: &str,
    component_key: &str,
) -> Result<Vec<Value>, String> {
    let permissions: &[(&str, &str)] = match profile {
        "restricted" => &[("process.spawn", "启动用户确认安装的本机 MCP Server")],
        "network_client" => &[
            ("process.spawn", "启动用户确认安装的本机 MCP Server"),
            ("network.client", "允许 MCP 发起出站网络请求"),
        ],
        "developer_analysis" => &[
            ("process.spawn", "启动用户确认安装的本机 MCP Server"),
            ("network.client", "允许分析工具发起出站网络请求"),
            ("system.developer-tools.read", "读取并调用本机开发工具链"),
            ("browser.managed.launch", "启动受控本机浏览器进行动态分析"),
            ("artifact.create", "在 ChatOS 管理目录中保存分析证据"),
            ("workspace.read", "读取当前任务绑定的项目文件"),
        ],
        _ => return Err(install_error("npm MCP access profile 无效")),
    };
    Ok(permissions
        .iter()
        .map(|(permission, reason)| {
            json!({
                "permission": permission,
                "required": true,
                "reason": reason,
                "components": [component_key]
            })
        })
        .collect())
}

pub(super) fn bounded_json_string(value: &Value, maximum: usize) -> Option<String> {
    let value = value.as_str()?;
    (!value.is_empty() && value.len() <= maximum && !value.contains('\0')).then(|| value.to_owned())
}

pub(super) fn npm_binary_projection(
    value: Option<&Value>,
    package_name: &str,
) -> Result<Value, String> {
    let Some(value) = value else {
        return Ok(json!({}));
    };
    if let Some(path) = value.as_str() {
        let binary_name = package_name.rsplit('/').next().unwrap_or(package_name);
        return Ok(json!({binary_name: bounded_npm_binary_path(path)?}));
    }
    let values = value
        .as_object()
        .ok_or_else(|| install_error("npm bin 元数据无效"))?;
    if values.len() > 64 {
        return Err(install_error("npm bin 元数据超过限制"));
    }
    let mut result = serde_json::Map::new();
    for (name, path) in values {
        validate_npm_binary_name(name)?;
        let path = path
            .as_str()
            .ok_or_else(|| install_error("npm bin 元数据无效"))?;
        result.insert(name.clone(), Value::String(bounded_npm_binary_path(path)?));
    }
    Ok(Value::Object(result))
}

fn bounded_npm_binary_path(value: &str) -> Result<String, String> {
    if value.is_empty() || value.len() > 1_024 || value.contains(['\0', '\r', '\n']) {
        Err(install_error("npm bin 路径无效"))
    } else {
        Ok(value.to_owned())
    }
}

fn validate_npm_binary_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Err(install_error("npm bin 名称无效"))
    } else {
        Ok(())
    }
}

pub(super) fn validate_npm_package_name(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 214 || value.trim() != value {
        return Err(install_error("npm 包名无效"));
    }
    let valid_part = |part: &str| {
        !part.is_empty()
            && part.as_bytes()[0].is_ascii_alphanumeric()
            && part.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
    };
    let valid = if let Some(scoped) = value.strip_prefix('@') {
        let Some((scope, name)) = scoped.split_once('/') else {
            return Err(install_error("npm 包名无效"));
        };
        !name.contains('/') && valid_part(scope) && valid_part(name)
    } else {
        !value.contains('/') && valid_part(value)
    };
    valid
        .then_some(())
        .ok_or_else(|| install_error("npm 包名无效"))
}

pub(super) fn validate_exact_version(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 || value.trim() != value {
        return Err(install_error("npm 版本必须是精确语义版本"));
    }
    let (without_build, build) = value
        .split_once('+')
        .map_or((value, None), |(left, right)| (left, Some(right)));
    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(left, right)| (left, Some(right)));
    let core_parts = core.split('.').collect::<Vec<_>>();
    let valid_identifier = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    };
    if core_parts.len() != 3
        || core_parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        || prerelease.is_some_and(|value| !valid_identifier(value))
        || build.is_some_and(|value| !valid_identifier(value))
    {
        return Err(install_error("npm 版本必须是精确语义版本"));
    }
    Ok(())
}
