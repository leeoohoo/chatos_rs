use super::*;

pub(super) fn validate_git_repository_url(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 4_096
        || value.contains(['\0', '\r', '\n'])
        || value.trim() != value
    {
        return Err(install_error("Git Plugin 来源无效"));
    }
    let url = reqwest::Url::parse(value).map_err(|_| install_error("Git Plugin 来源无效"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(install_error(
            "Git Plugin 只接受不含凭据、查询参数或片段的 HTTPS 仓库 URL",
        ));
    }
    Ok(())
}

pub(super) fn validate_git_reference(value: Option<&str>) -> Result<String, String> {
    let value = value.unwrap_or("HEAD");
    if value.is_empty()
        || value.len() > 256
        || value.starts_with('-')
        || value.contains("..")
        || value.contains("@{")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b'-'))
    {
        return Err(install_error("Git Plugin ref 无效"));
    }
    Ok(value.to_owned())
}

#[cfg(windows)]
pub(super) fn git_null_hooks_config() -> &'static str {
    "core.hooksPath=NUL"
}

#[cfg(not(windows))]
pub(super) fn git_null_hooks_config() -> &'static str {
    "core.hooksPath=/dev/null"
}

pub(super) async fn run_plugin_git(
    arguments: &[&str],
    directory: &Path,
    failure_message: &str,
) -> Result<(), String> {
    let mut command = plugin_git_command(arguments, directory);
    command.stdout(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|_| install_error("无法启动 Git；请先安装 Git for Windows"))?;
    let termination = ProcessTermination::attach(&child).map_err(|_| {
        let _ = child.start_kill();
        install_error("无法隔离 Git Plugin 获取进程")
    })?;
    let status = match timeout(GIT_OPERATION_TIMEOUT, child.wait()).await {
        Ok(result) => result.map_err(|_| install_error("Git Plugin 获取失败"))?,
        Err(_) => {
            terminate_git_process(&mut child, &termination).await;
            return Err(install_error("Git Plugin 获取超过 5 分钟，已终止"));
        }
    };
    let _ = termination.terminate(true);
    if !status.success() {
        return Err(install_error(&format!(
            "{failure_message}（Git 退出码 {}）",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

pub(super) async fn run_plugin_git_archive(
    directory: &Path,
    destination: &Path,
) -> Result<(), String> {
    let mut command = plugin_git_command(&["archive", "--format=tar", "FETCH_HEAD"], directory);
    command.stdout(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| install_error("无法启动 Git；请先安装 Git for Windows"))?;
    let termination = ProcessTermination::attach(&child).map_err(|_| {
        let _ = child.start_kill();
        install_error("无法隔离 Git Plugin 获取进程")
    })?;
    let Some(mut stdout) = child.stdout.take() else {
        terminate_git_process(&mut child, &termination).await;
        return Err(install_error("无法读取 Git Plugin 快照"));
    };
    let mut options = tokio::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(windows)]
    {
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let output = options.open(destination).await;
    let mut output = match output {
        Ok(output) => output,
        Err(_) => {
            terminate_git_process(&mut child, &termination).await;
            return Err(install_error("无法保存 Git Plugin 快照"));
        }
    };
    let operation = async {
        let mut written = 0_u64;
        let mut buffer = vec![0_u8; 64 * 1_024];
        loop {
            let count = stdout
                .read(&mut buffer)
                .await
                .map_err(|_| install_error("Git Plugin 快照读取失败"))?;
            if count == 0 {
                break;
            }
            written = written
                .checked_add(count as u64)
                .filter(|value| *value <= MAXIMUM_ARCHIVE_BYTES)
                .ok_or_else(|| install_error("Git Plugin 快照大小超过限制"))?;
            output
                .write_all(&buffer[..count])
                .await
                .map_err(|_| install_error("无法保存 Git Plugin 快照"))?;
        }
        output
            .flush()
            .await
            .map_err(|_| install_error("无法保存 Git Plugin 快照"))?;
        output
            .sync_all()
            .await
            .map_err(|_| install_error("无法保存 Git Plugin 快照"))?;
        let status = child
            .wait()
            .await
            .map_err(|_| install_error("无法冻结 Git Plugin 仓库快照"))?;
        if written == 0 {
            return Err(install_error("Git Plugin 快照为空"));
        }
        Ok(status)
    };
    let result = match timeout(GIT_OPERATION_TIMEOUT, operation).await {
        Ok(result) => result,
        Err(_) => Err(install_error("Git Plugin 快照超过 5 分钟，已终止")),
    };
    if result.is_err() {
        terminate_git_process(&mut child, &termination).await;
        let _ = tokio::fs::remove_file(destination).await;
    } else {
        let _ = termination.terminate(true);
    }
    let status = result?;
    if !status.success() {
        let _ = tokio::fs::remove_file(destination).await;
        return Err(install_error(&format!(
            "无法冻结 Git Plugin 仓库快照（Git 退出码 {}）",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

fn plugin_git_command(arguments: &[&str], directory: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(directory)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", git_null_device())
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
    }
    command
}

async fn terminate_git_process(
    child: &mut tokio::process::Child,
    termination: &ProcessTermination,
) {
    let _ = termination.terminate(false);
    if timeout(PROCESS_TERMINATION_GRACE, child.wait())
        .await
        .is_err()
    {
        let _ = termination.terminate(true);
        let _ = timeout(PROCESS_TERMINATION_GRACE, child.wait()).await;
    }
}

#[cfg(windows)]
fn git_null_device() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
fn git_null_device() -> &'static str {
    "/dev/null"
}

pub(super) fn extract_git_archive(archive_path: &Path, package_root: &Path) -> Result<(), String> {
    ensure_owned_directory(package_root)
        .map_err(|_| install_error("无法创建 Git Plugin 解包目录"))?;
    let file = File::open(archive_path).map_err(|_| install_error("无法读取 Git Plugin 快照"))?;
    let mut archive = Archive::new(file);
    let mut observed = BTreeSet::new();
    let mut count = 0_usize;
    let mut total_bytes = 0_u64;
    for entry in archive
        .entries()
        .map_err(|_| install_error("Git Plugin 快照格式无效"))?
    {
        let mut entry = entry.map_err(|_| install_error("Git Plugin 快照格式无效"))?;
        count += 1;
        if count > MAXIMUM_ARCHIVE_ENTRIES {
            return Err(install_error("Git Plugin 文件数量超过限制"));
        }
        let kind = entry.header().entry_type();
        if matches!(
            kind,
            tar::EntryType::XGlobalHeader
                | tar::EntryType::XHeader
                | tar::EntryType::GNULongName
                | tar::EntryType::GNULongLink
        ) {
            if entry.size() > 64 * 1_024 {
                return Err(install_error("Git Plugin 元数据超过限制"));
            }
            continue;
        }
        let path = entry
            .path()
            .map_err(|_| install_error("Git Plugin 路径无效"))?
            .into_owned();
        let relative = safe_relative_path(&path)?;
        if relative.as_os_str().len() > 4_096 {
            return Err(install_error("Git Plugin 路径过长"));
        }
        if !observed.insert(relative_path_identity(&relative)?) {
            return Err(install_error("Git Plugin 包含大小写冲突路径"));
        }
        if kind.is_file() {
            total_bytes = total_bytes
                .checked_add(entry.size())
                .filter(|value| *value <= MAXIMUM_UNPACKED_BYTES)
                .ok_or_else(|| install_error("Git Plugin 体积超过限制"))?;
        } else if !kind.is_dir() {
            return Err(install_error("Git Plugin 包含链接或特殊文件"));
        }
        let destination = package_root.join(&relative);
        if kind.is_dir() {
            ensure_owned_directory(&destination)
                .map_err(|_| install_error("无法创建 Git Plugin 目录"))?;
        } else {
            if let Some(parent) = destination.parent() {
                ensure_owned_directory(parent)
                    .map_err(|_| install_error("无法创建 Git Plugin 目录"))?;
            }
            entry
                .unpack(&destination)
                .map_err(|_| install_error("无法解包 Git Plugin 文件"))?;
        }
    }
    if observed.is_empty() {
        return Err(install_error("Git Plugin 仓库为空"));
    }
    Ok(())
}
