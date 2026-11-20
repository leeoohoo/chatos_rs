use super::*;

pub(super) async fn prepare_program_mcp_package(
    plugin_root: &Path,
    draft: &ProgramMcpDraft,
) -> Result<AcquiredPluginSource, String> {
    validate_program_mcp_draft(draft)?;
    let plugin_root = canonical_plugin_root(plugin_root)?;
    let staging_root = plugin_root
        .join(".source-staging")
        .join(Uuid::new_v4().to_string());
    let package_root = staging_root.join("package");
    let runtime_root = package_root.join("runtime");
    let bin_root = package_root.join("bin");
    ensure_owned_directory(&runtime_root)
        .map_err(|_| install_error("无法创建程序 MCP 临时目录"))?;
    ensure_owned_directory(&bin_root).map_err(|_| install_error("无法创建程序 MCP 启动目录"))?;
    let acquired = AcquiredPluginSource {
        plugin_root,
        staging_root,
        package_root,
    };
    let source_summary;
    let executable_path = if draft.kind == "github_release" {
        let asset_name = draft
            .asset_name
            .as_deref()
            .ok_or_else(|| install_error("GitHub Release 程序 MCP 缺少 asset_name"))?;
        let expected_digest = draft
            .archive_sha256
            .as_deref()
            .ok_or_else(|| install_error("请先解析官方发行包，取得固定 SHA-256"))?;
        let (archive, digest, _) =
            download_release_archive(&draft.location, &draft.exact_version, asset_name).await?;
        if digest != expected_digest {
            return Err(install_error("发行包在解析后发生变化，请重新解析和预检"));
        }
        extract_release_archive(&archive, asset_name, &runtime_root)?;
        source_summary = format!(
            "GitHub Release · {} · {} · {}",
            draft.location, draft.exact_version, asset_name
        );
        runtime_root.join(&draft.executable_path)
    } else {
        let source = Path::new(&draft.location)
            .canonicalize()
            .map_err(|_| install_error("本机 MCP 程序路径无效"))?;
        let metadata =
            fs::symlink_metadata(&source).map_err(|_| install_error("本机 MCP 程序不存在"))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(install_error("本机 MCP 程序必须是普通文件"));
        }
        let target = runtime_root.join("program.exe");
        fs::copy(&source, &target).map_err(|_| install_error("无法复制本机 MCP 程序"))?;
        source_summary = format!(
            "Local executable · {} · {}",
            source.display(),
            draft.exact_version
        );
        target
    };
    let runtime_root = runtime_root
        .canonicalize()
        .map_err(|_| install_error("程序 MCP 运行目录无效"))?;
    let executable_path = executable_path
        .canonicalize()
        .map_err(|_| install_error("程序 MCP 可执行入口不存在"))?;
    if !executable_path.starts_with(&runtime_root)
        || !executable_path.is_file()
        || !executable_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err(install_error(
            "程序 MCP 可执行入口必须是归档内的 Windows .exe 文件",
        ));
    }
    let launcher = bin_root.join(format!("{}.exe", draft.component_key));
    fs::copy(&executable_path, &launcher)
        .map_err(|_| install_error("无法冻结程序 MCP 可执行入口"))?;
    write_json_file(
        &acquired.package_root.join("chatos.plugin.json"),
        &json!({
            "schemaVersion": 3,
            "name": format!("program-mcp-{}", &sha256_text(&source_summary)[..16]),
            "version": draft.exact_version,
            "description": source_summary,
            "author": {"name": "Local"},
            "interface": {
                "displayName": draft.display_name,
                "shortDescription": source_summary,
                "longDescription": source_summary,
                "developerName": "Local",
                "category": "Developer Tools"
            },
            "mcpServers": {
                draft.component_key.clone(): {
                    "type": "stdio",
                    "bin": draft.component_key,
                    "args": draft.arguments,
                }
            },
            "permissions": npm_access_permissions(&draft.access_profile, &draft.component_key)?
        }),
    )?;
    validate_local_package(&acquired.package_root)?;
    Ok(acquired)
}

pub(super) async fn inspect_prepared_program_mcp(
    package_root: &Path,
    draft: &ProgramMcpDraft,
) -> Result<ProgramMcpInspection, String> {
    let local = inspect_local_package(package_root, None)?;
    let server = local
        .manifest
        .mcp_servers
        .iter()
        .find(|server| server.component_key() == draft.component_key)
        .ok_or_else(|| install_error("程序 MCP 组件清单无效"))?;
    let PluginMcpServer::Stdio { bin, args, env, .. } = server else {
        return Err(install_error("程序 MCP 必须使用 stdio transport"));
    };
    let executable = resolve_executable(package_root, bin)?;
    let tools = inspect_mcp_tools(package_root, &executable, args, env).await?;
    let fingerprint = program_mcp_fingerprint(&local.fingerprint, draft, &tools);
    Ok(ProgramMcpInspection {
        fingerprint,
        display_name: draft.display_name.clone(),
        version: draft.exact_version.clone(),
        source_summary: draft.location.clone(),
        component_key: draft.component_key.clone(),
        tools,
        content_fingerprint: local.fingerprint,
    })
}

fn validate_program_mcp_draft(draft: &ProgramMcpDraft) -> Result<(), String> {
    if !matches!(draft.kind.as_str(), "github_release" | "local_executable")
        || draft.display_name.trim().is_empty()
        || draft.display_name.len() > 120
        || !valid_executable_name(&draft.component_key)
        || draft.exact_version.is_empty()
        || draft.exact_version.len() > 256
        || draft.exact_version.contains(['\0', '/', '\\'])
        || draft.arguments.len() > 64
        || draft
            .arguments
            .iter()
            .any(|arg| arg.len() > 2_048 || arg.contains('\0'))
        || !matches!(
            draft.access_profile.as_str(),
            "restricted" | "network_client" | "developer_analysis"
        )
    {
        return Err(install_error("程序 MCP 安装参数无效"));
    }
    if draft.kind == "github_release"
        && (!draft.location.starts_with("https://github.com/")
            || draft
                .archive_sha256
                .as_deref()
                .is_none_or(|value| !valid_sha256(value)))
    {
        return Err(install_error("GitHub Release 程序 MCP 来源或 SHA-256 无效"));
    }
    if draft.kind == "local_executable" && !Path::new(&draft.location).is_absolute() {
        return Err(install_error("本机 MCP 程序必须使用绝对路径"));
    }
    Ok(())
}

fn program_mcp_fingerprint(
    content_fingerprint: &str,
    draft: &ProgramMcpDraft,
    tools: &[Value],
) -> String {
    let payload = json!({
        "content_fingerprint": content_fingerprint,
        "kind": draft.kind,
        "location": draft.location,
        "version": draft.exact_version,
        "display_name": draft.display_name,
        "component_key": draft.component_key,
        "executable_path": draft.executable_path,
        "arguments": draft.arguments,
        "access_profile": draft.access_profile,
        "asset_name": draft.asset_name,
        "archive_sha256": draft.archive_sha256,
        "tools": tools,
    });
    sha256_text(&serde_json::to_string(&payload).unwrap_or_default())
}
