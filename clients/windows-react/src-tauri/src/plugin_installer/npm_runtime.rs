use super::*;

pub(super) fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| install_error("无法生成 npm MCP 安装清单"))?;
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options
        .open(path)
        .map_err(|_| install_error("无法保存 npm MCP 安装清单"))?;
    file.write_all(&bytes)
        .map_err(|_| install_error("无法保存 npm MCP 安装清单"))?;
    file.sync_all()
        .map_err(|_| install_error("无法保存 npm MCP 安装清单"))
}

pub(super) fn create_private_file(path: &Path) -> std::io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

pub(super) fn find_system_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    for directory in std::env::split_paths(&path) {
        for extension in extensions {
            let candidate = directory.join(format!("{name}{extension}"));
            let Ok(metadata) = fs::symlink_metadata(&candidate) else {
                continue;
            };
            if !metadata.is_file() || is_symlink_or_reparse(&metadata) {
                continue;
            }
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if canonical.is_file() {
                return Some(external_process_path(&canonical));
            }
        }
    }
    None
}

pub(super) fn resolve_npm_cli(node: &Path) -> Result<PathBuf, String> {
    let parent = node
        .parent()
        .ok_or_else(|| install_error("Node.js 安装目录无效"))?;
    let candidates = [
        parent.join("node_modules/npm/bin/npm-cli.js"),
        parent.join("../lib/node_modules/npm/bin/npm-cli.js"),
    ];
    for candidate in candidates {
        let Ok(metadata) = fs::symlink_metadata(&candidate) else {
            continue;
        };
        if metadata.is_file() && !is_symlink_or_reparse(&metadata) {
            let canonical = candidate
                .canonicalize()
                .map_err(|_| install_error("npm CLI 路径无效"))?;
            if canonical.is_file() {
                return Ok(external_process_path(&canonical));
            }
        }
    }
    Err(install_error("没有找到与 Node.js 配套的 npm CLI"))
}

#[cfg(windows)]
pub(super) fn external_process_path(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(value) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{value}"));
    }
    value
        .strip_prefix(r"\\?\")
        .map(PathBuf::from)
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(not(windows))]
pub(super) fn external_process_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

pub(super) async fn run_isolated_npm(
    node: &Path,
    npm_cli: &Path,
    package_root: &Path,
    staging_root: &Path,
    user_config: &Path,
    global_config: &Path,
    arguments: &[&str],
) -> Result<(), String> {
    let node_directory = node
        .parent()
        .ok_or_else(|| install_error("Node.js 安装目录无效"))?;
    let cache = staging_root.join("npm-cache");
    ensure_owned_directory(&cache).map_err(|_| install_error("无法创建隔离 npm 缓存"))?;
    let mut command = Command::new(node);
    command
        .arg(npm_cli)
        .args(arguments)
        .current_dir(package_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false)
        .env_clear()
        .env("PATH", node_directory)
        .env("npm_config_userconfig", user_config)
        .env("npm_config_globalconfig", global_config)
        .env("npm_config_cache", cache)
        .env("npm_config_registry", "https://registry.npmjs.org/")
        .env("npm_config_ignore_scripts", "true")
        .env("npm_config_audit", "false")
        .env("npm_config_fund", "false")
        .env("npm_config_update_notifier", "false")
        .env("npm_config_fetch_retries", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never");
    for key in ["SystemRoot", "WINDIR", "ComSpec", "PATHEXT", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|_| install_error("无法启动隔离 npm 安装"))?;
    let termination = ProcessTermination::attach(&child).map_err(|_| {
        let _ = child.start_kill();
        install_error("无法隔离 npm 安装进程")
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| install_error("无法读取 npm 安装输出"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| install_error("无法读取 npm 安装错误"))?;
    let stdout_task = tokio::spawn(read_bounded_process_output(stdout));
    let stderr_task = tokio::spawn(read_bounded_process_output(stderr));
    let status = match timeout(NPM_INSTALL_TIMEOUT, child.wait()).await {
        Ok(result) => result.map_err(|_| install_error("npm 安装进程异常退出"))?,
        Err(_) => {
            terminate_child(&mut child, &termination).await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(install_error("npm 安装超过 5 分钟，已终止"));
        }
    };
    let _ = termination.terminate(true);
    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        #[cfg(test)]
        eprintln!(
            "isolated npm diagnostic: {}",
            safe_process_diagnostic(&stderr, &stdout)
        );
        #[cfg(not(test))]
        let _ = (stdout, stderr);
        Err(install_error(&format!(
            "npm 安装失败（退出码 {}）",
            status.code().unwrap_or(-1)
        )))
    }
}

pub(super) async fn read_bounded_process_output(mut pipe: impl AsyncRead + Unpin) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        let Ok(count) = pipe.read(&mut buffer).await else {
            return output;
        };
        if count == 0 {
            return output;
        }
        let remaining = MAXIMUM_NPM_PROCESS_OUTPUT_BYTES.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..count.min(remaining)]);
    }
}

#[cfg(test)]
pub(super) fn safe_process_diagnostic(stderr: &[u8], stdout: &[u8]) -> String {
    let source = if stderr.is_empty() { stdout } else { stderr };
    let sanitized = String::from_utf8_lossy(source)
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let lines = sanitized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(12)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "无可用诊断".into()
    } else {
        lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
    }
}

pub(super) fn validate_npm_lock(path: &Path) -> Result<(), String> {
    let raw = read_bounded_text(path, MAXIMUM_NPM_LOCK_BYTES)?;
    let lock: Value = serde_json::from_str(&raw).map_err(|_| install_error("npm lockfile 无效"))?;
    if lock.get("lockfileVersion").and_then(Value::as_u64) != Some(3) {
        return Err(install_error("npm lockfile 版本不受支持"));
    }
    let packages = lock
        .get("packages")
        .and_then(Value::as_object)
        .filter(|packages| !packages.is_empty() && packages.len() <= MAXIMUM_ARCHIVE_ENTRIES)
        .ok_or_else(|| install_error("npm lockfile 包数量无效"))?;
    for (path, package) in packages {
        if path.is_empty() {
            continue;
        }
        let package = package
            .as_object()
            .ok_or_else(|| install_error("npm lockfile 包记录无效"))?;
        if package.get("link").and_then(Value::as_bool) == Some(true) {
            return Err(install_error("npm lockfile 包含链接依赖"));
        }
        let resolved = package
            .get("resolved")
            .and_then(Value::as_str)
            .ok_or_else(|| install_error("npm lockfile 缺少固定 tarball"))?;
        validate_npm_tarball_url(resolved)?;
        let integrity = package
            .get("integrity")
            .and_then(Value::as_str)
            .ok_or_else(|| install_error("npm lockfile 缺少 sha512 integrity"))?;
        parse_npm_integrity(integrity)?;
    }
    Ok(())
}

pub(super) fn sanitize_npm_tree(root: &Path) -> Result<(), String> {
    let mut visited = 0usize;
    let shim_directory = root.join("node_modules").join(".bin");
    if shim_directory.exists() {
        remove_tree_without_following_links(&shim_directory, &mut visited)?;
    }
    sanitize_npm_directory(root, &mut visited)
}

pub(super) fn sanitize_npm_directory(directory: &Path, visited: &mut usize) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|_| install_error("无法检查 npm 安装目录"))?
    {
        let entry = entry.map_err(|_| install_error("无法检查 npm 安装目录"))?;
        *visited = visited.saturating_add(1);
        if *visited > MAXIMUM_ARCHIVE_ENTRIES {
            return Err(install_error("npm 安装目录条目超过限制"));
        }
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| install_error("无法检查 npm 安装文件"))?;
        if is_symlink_or_reparse(&metadata) {
            if metadata.is_dir() {
                fs::remove_dir(&path).map_err(|_| install_error("无法移除 npm 链接目录"))?;
            } else {
                fs::remove_file(&path).map_err(|_| install_error("无法移除 npm 链接文件"))?;
            }
        } else if metadata.is_dir() {
            sanitize_npm_directory(&path, visited)?;
        } else if !metadata.is_file() {
            return Err(install_error("npm 安装包含特殊文件"));
        }
    }
    Ok(())
}

pub(super) fn remove_tree_without_following_links(
    path: &Path,
    visited: &mut usize,
) -> Result<(), String> {
    *visited = visited.saturating_add(1);
    if *visited > MAXIMUM_ARCHIVE_ENTRIES {
        return Err(install_error("npm shim 目录条目超过限制"));
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| install_error("无法清理 npm shim"))?;
    if is_symlink_or_reparse(&metadata) {
        return if metadata.is_dir() {
            fs::remove_dir(path).map_err(|_| install_error("无法清理 npm shim"))
        } else {
            fs::remove_file(path).map_err(|_| install_error("无法清理 npm shim"))
        };
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|_| install_error("无法清理 npm shim"))? {
            remove_tree_without_following_links(
                &entry
                    .map_err(|_| install_error("无法清理 npm shim"))?
                    .path(),
                visited,
            )?;
        }
        fs::remove_dir(path).map_err(|_| install_error("无法清理 npm shim"))
    } else {
        fs::remove_file(path).map_err(|_| install_error("无法清理 npm shim"))
    }
}

pub(super) fn npm_mcp_fingerprint(
    content_fingerprint: &str,
    draft: &NpmMcpDraft,
    tools: &[Value],
) -> Result<String, String> {
    let mut tools = tools.to_vec();
    tools.sort_by(|left, right| {
        left.get("name")
            .and_then(Value::as_str)
            .cmp(&right.get("name").and_then(Value::as_str))
    });
    let draft = json!({
        "display_name": draft.display_name,
        "description": draft.description,
        "component_key": draft.component_key,
        "package_name": draft.package_name,
        "package_version": draft.package_version,
        "binary_name": draft.binary_name,
        "arguments": draft.arguments,
        "access_profile": draft.access_profile
    });
    let encoded_draft =
        serde_json::to_vec(&draft).map_err(|_| install_error("无法冻结 npm MCP 安装请求"))?;
    let encoded_tools =
        serde_json::to_vec(&tools).map_err(|_| install_error("无法冻结 npm MCP 工具清单"))?;
    let mut digest = Sha256::new();
    digest.update(content_fingerprint.as_bytes());
    digest.update([0]);
    digest.update(encoded_draft);
    digest.update([0]);
    digest.update(encoded_tools);
    Ok(format!("{:x}", digest.finalize()))
}
