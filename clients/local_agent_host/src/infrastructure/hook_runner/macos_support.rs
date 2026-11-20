#[cfg(target_os = "macos")]
async fn read_bounded<R>(mut reader: R, limit: usize) -> Result<(Vec<u8>, bool), String>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::with_capacity(limit.min(16 * 1_024));
    let mut overflow = false;
    let mut chunk = [0_u8; 8 * 1_024];
    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|error| format!("read Hook output failed: {error}"))?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&chunk[..count.min(remaining)]);
        overflow |= count > remaining;
    }
    Ok((output, overflow))
}

#[cfg(target_os = "macos")]
fn hook_environment(
    installation_root: &Path,
    data_root: &Path,
    temporary_root: &Path,
    workspace_root: Option<&Path>,
) -> BTreeMap<String, String> {
    let project = workspace_root.unwrap_or(installation_root);
    BTreeMap::from([
        (
            "PATH".to_string(),
            "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin".to_string(),
        ),
        ("HOME".to_string(), data_root.display().to_string()),
        ("TMPDIR".to_string(), temporary_root.display().to_string()),
        (
            "PLUGIN_ROOT".to_string(),
            installation_root.display().to_string(),
        ),
        ("PLUGIN_DATA".to_string(), data_root.display().to_string()),
        (
            "CLAUDE_PLUGIN_ROOT".to_string(),
            installation_root.display().to_string(),
        ),
        (
            "CLAUDE_PLUGIN_DATA".to_string(),
            data_root.display().to_string(),
        ),
        (
            "CLAUDE_PROJECT_DIR".to_string(),
            project.display().to_string(),
        ),
        (
            "CHATOS_PLUGIN_SANDBOX".to_string(),
            "macos-seatbelt-hook-v1".to_string(),
        ),
    ])
}

#[cfg(target_os = "macos")]
fn canonical_directory(path: &Path, name: &str) -> Result<PathBuf, String> {
    let canonical =
        std::fs::canonicalize(path).map_err(|error| format!("{name} is unavailable: {error}"))?;
    if !canonical.is_dir() {
        return Err(format!("{name} is not a directory"));
    }
    Ok(canonical)
}

#[cfg(target_os = "macos")]
fn seatbelt_profile(
    installation_root: &Path,
    data_root: &Path,
    workspace_root: Option<&Path>,
    permissions: &[String],
) -> String {
    let has = |permission: &str| permissions.iter().any(|value| value == permission);
    let mut rules = vec![
        "(version 1)".to_string(),
        "(deny default)".to_string(),
        "(import \"system.sb\")".to_string(),
        format!(
            "(allow process-exec (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/opt/homebrew\") (subpath \"/usr/local\") {})",
            seatbelt_literal(installation_root)
        ),
        "(allow process-fork)".to_string(),
        "(allow signal (target same-sandbox))".to_string(),
        "(allow sysctl-read)".to_string(),
        "(allow mach-lookup (global-name \"com.apple.cfprefsd.agent\") (global-name \"com.apple.system.logger\") (global-name \"com.apple.system.notification_center\") (global-name \"com.apple.securityd\") (global-name \"com.apple.system.opendirectoryd.libinfo\"))".to_string(),
        "(allow file-read-metadata)".to_string(),
        format!(
            "(allow file-read* (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/opt/homebrew\") (subpath \"/usr/local\") (subpath \"/Library/Apple\") {})",
            seatbelt_literal(installation_root)
        ),
        format!(
            "(allow file-read* file-write* {})",
            seatbelt_literal(data_root)
        ),
    ];
    if let Some(workspace_root) = workspace_root {
        if has("workspace.write") {
            rules.push(format!(
                "(allow file-read* file-write* {})",
                seatbelt_literal(workspace_root)
            ));
        } else if has("workspace.read") {
            rules.push(format!(
                "(allow file-read* {})",
                seatbelt_literal(workspace_root)
            ));
        }
    }
    if has("network.client") {
        rules.push("(allow network-outbound)".to_string());
    }
    rules.join("\n")
}

#[cfg(target_os = "macos")]
fn seatbelt_literal(path: &Path) -> String {
    let escaped = path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("(subpath \"{escaped}\")")
}

#[cfg(target_os = "macos")]
struct ProcessGroupGuard {
    process_id: u32,
    armed: bool,
}

#[cfg(target_os = "macos")]
impl ProcessGroupGuard {
    fn new(process_id: u32) -> Self {
        Self {
            process_id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

#[cfg(target_os = "macos")]
impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if self.armed {
            kill_process_group(self.process_id);
        }
    }
}

#[cfg(target_os = "macos")]
fn terminate_process_group(process_id: u32) {
    unsafe {
        libc::kill(-(process_id as i32), libc::SIGTERM);
    }
}

#[cfg(target_os = "macos")]
fn kill_process_group(process_id: u32) {
    unsafe {
        libc::kill(-(process_id as i32), libc::SIGKILL);
    }
}
