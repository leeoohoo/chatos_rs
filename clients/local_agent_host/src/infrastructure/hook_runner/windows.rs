#[cfg(windows)]
const WINDOWS_MAXIMUM_STDOUT_BYTES: usize = 512 * 1_024;
#[cfg(windows)]
const WINDOWS_MAXIMUM_STDERR_BYTES: usize = 64 * 1_024;
#[cfg(windows)]
const WINDOWS_MAXIMUM_STDIN_BYTES: usize = 256 * 1_024;

#[cfg(windows)]
#[derive(Clone)]
pub struct WindowsAppContainerHookRunner {
    data_root: PathBuf,
    leases: Arc<crate::infrastructure::WorkspaceLeaseManager>,
    execution_lock: Arc<Mutex<()>>,
}

#[cfg(windows)]
impl WindowsAppContainerHookRunner {
    pub fn new(data_root: PathBuf) -> Result<Self, String> {
        if !data_root.is_absolute() {
            return Err("Hook data root must be absolute".to_string());
        }
        std::fs::create_dir_all(&data_root)
            .map_err(|error| format!("create Windows Hook data root failed: {error}"))?;
        let data_root = data_root
            .canonicalize()
            .map_err(|error| format!("resolve Windows Hook data root failed: {error}"))?;
        let leases = crate::infrastructure::WorkspaceLeaseManager::open(&data_root)?;
        Ok(Self {
            data_root,
            leases: Arc::new(leases),
            execution_lock: Arc::new(Mutex::new(())),
        })
    }

    fn plugin_data_path(&self, bundle: &LocalPluginHookBundleSpec) -> PathBuf {
        let mut digest = Sha256::new();
        for value in [
            bundle.owner_user_id.as_str(),
            bundle.plugin_id.as_str(),
            bundle.release_id.as_str(),
        ] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }
        self.data_root.join(format!("{:x}", digest.finalize()))
    }
}

#[cfg(windows)]
#[async_trait]
impl LocalHookRunner for WindowsAppContainerHookRunner {
    async fn run(
        &self,
        invocation: LocalHookInvocation,
    ) -> Result<LocalHookInvocationResult, String> {
        invocation.bundle.validate()?;
        if !invocation.bundle.hooks.contains(&invocation.hook) {
            return Err("Hook invocation is not part of its trusted bundle".to_string());
        }
        if invocation.stdin_json.len() > WINDOWS_MAXIMUM_STDIN_BYTES {
            return Err("Hook stdin exceeds 262144 bytes".to_string());
        }
        let installation_root = PathBuf::from(&invocation.bundle.installation_root);
        let installation_root = installation_root
            .canonicalize()
            .map_err(|error| format!("Hook installation root is unavailable: {error}"))?;
        let data_root = self.plugin_data_path(&invocation.bundle);
        let temporary_root = data_root.join("tmp");
        std::fs::create_dir_all(&temporary_root)
            .map_err(|error| format!("create Windows Hook data directory failed: {error}"))?;
        let workspace_root = invocation
            .workspace_root
            .as_deref()
            .map(|path| {
                path.canonicalize()
                    .map_err(|error| format!("Hook workspace root is unavailable: {error}"))
            })
            .transpose()?;
        let mut workspace_leases = vec![
            (
                installation_root.clone(),
                crate::infrastructure::WorkspaceAccess::ReadOnly,
            ),
            (
                data_root.clone(),
                crate::infrastructure::WorkspaceAccess::ReadWrite,
            ),
        ];
        if let Some(workspace_root) = workspace_root.clone() {
            let access = if has_permission(&invocation.bundle.permissions, "workspace.write") {
                crate::infrastructure::WorkspaceAccess::ReadWrite
            } else {
                crate::infrastructure::WorkspaceAccess::ReadOnly
            };
            workspace_leases.push((workspace_root, access));
        }
        let current_directory = workspace_root
            .clone()
            .unwrap_or_else(|| installation_root.clone());
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let command_executable = system_root.join("System32").join("cmd.exe");
        if !command_executable.is_file() {
            return Err("Windows command interpreter is unavailable".to_string());
        }
        let network_policy = if has_permission(&invocation.bundle.permissions, "network.client") {
            "controlled"
        } else {
            "disabled"
        };
        let environment = windows_hook_environment(
            &installation_root,
            &data_root,
            &temporary_root,
            &current_directory,
        );
        let leases = Arc::clone(&self.leases);
        let execution_lock = Arc::clone(&self.execution_lock);
        let stdin_json = invocation.stdin_json;
        let command = invocation.hook.command.clone();
        let timeout = Duration::from_millis(invocation.hook.timeout_ms);
        let started = Instant::now();
        let result = tokio::task::spawn_blocking(move || {
            let _execution_guard = execution_lock
                .lock()
                .map_err(|_| "Windows Hook execution lock is poisoned".to_string())?;
            run_windows_hook_process(
                &leases,
                &current_directory,
                &command_executable,
                &["/D".to_owned(), "/S".to_owned(), "/C".to_owned(), command],
                &workspace_leases,
                network_policy,
                &environment,
                &stdin_json,
                timeout,
            )
        })
        .await
        .map_err(|error| format!("Windows Hook worker failed: {error}"))??;
        Ok(LocalHookInvocationResult {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            stdout_overflow: result.stdout_overflow,
            stderr_overflow: result.stderr_overflow,
            timed_out: result.timed_out,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }
}

#[cfg(windows)]
struct WindowsHookProcessResult {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_overflow: bool,
    stderr_overflow: bool,
    timed_out: bool,
}

#[cfg(windows)]
fn run_windows_hook_process(
    leases: &crate::infrastructure::WorkspaceLeaseManager,
    current_directory: &Path,
    executable: &Path,
    arguments: &[String],
    workspace_leases: &[(PathBuf, crate::infrastructure::WorkspaceAccess)],
    network_policy: &str,
    environment: &[(OsString, OsString)],
    stdin_json: &[u8],
    timeout: Duration,
) -> Result<WindowsHookProcessResult, String> {
    let mut child = leases.spawn_workspace_command_with_leases(
        current_directory,
        executable,
        arguments,
        workspace_leases,
        network_policy,
        environment,
    )?;
    let mut stdin = child
        .take_stdin()
        .ok_or_else(|| "Windows Hook stdin is unavailable".to_string())?;
    let mut stdout = child
        .take_stdout()
        .ok_or_else(|| "Windows Hook stdout is unavailable".to_string())?;
    let mut stderr = child
        .take_stderr()
        .ok_or_else(|| "Windows Hook stderr is unavailable".to_string())?;
    let stdout_reader =
        thread::spawn(move || read_windows_hook_output(&mut stdout, WINDOWS_MAXIMUM_STDOUT_BYTES));
    let stderr_reader =
        thread::spawn(move || read_windows_hook_output(&mut stderr, WINDOWS_MAXIMUM_STDERR_BYTES));
    stdin
        .write_all(stdin_json)
        .map_err(|error| format!("write Windows Hook stdin failed: {error}"))?;
    drop(stdin);
    let wait = child.wait(timeout);
    let (stdout, stdout_overflow) = stdout_reader
        .join()
        .map_err(|_| "Windows Hook stdout reader panicked".to_string())??;
    let (stderr, stderr_overflow) = stderr_reader
        .join()
        .map_err(|_| "Windows Hook stderr reader panicked".to_string())??;
    match wait {
        Ok(exit_code) => Ok(WindowsHookProcessResult {
            exit_code: Some(exit_code),
            stdout,
            stderr,
            stdout_overflow,
            stderr_overflow,
            timed_out: false,
        }),
        Err(error) if error.contains("timed out") => Ok(WindowsHookProcessResult {
            exit_code: None,
            stdout,
            stderr,
            stdout_overflow,
            stderr_overflow,
            timed_out: true,
        }),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn read_windows_hook_output(
    reader: &mut impl Read,
    limit: usize,
) -> Result<(Vec<u8>, bool), String> {
    let mut output = Vec::with_capacity(limit.min(16 * 1_024));
    let mut overflow = false;
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("read Windows Hook output failed: {error}"))?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..count.min(remaining)]);
        overflow |= count > remaining;
    }
    Ok((output, overflow))
}

#[cfg(windows)]
fn has_permission(permissions: &[String], permission: &str) -> bool {
    permissions.iter().any(|value| value == permission)
}

#[cfg(windows)]
fn windows_hook_environment(
    installation_root: &Path,
    data_root: &Path,
    temporary_root: &Path,
    workspace_root: &Path,
) -> Vec<(OsString, OsString)> {
    let mut names = BTreeSet::new();
    let mut environment = chatos_windows_appcontainer::restricted_runtime_environment();
    for (name, _) in &environment {
        names.insert(name.to_string_lossy().to_lowercase());
    }
    for (name, value) in [
        ("HOME", data_root.as_os_str()),
        ("TEMP", temporary_root.as_os_str()),
        ("TMP", temporary_root.as_os_str()),
        ("PLUGIN_ROOT", installation_root.as_os_str()),
        ("PLUGIN_DATA", data_root.as_os_str()),
        ("CLAUDE_PLUGIN_ROOT", installation_root.as_os_str()),
        ("CLAUDE_PLUGIN_DATA", data_root.as_os_str()),
        ("CLAUDE_PROJECT_DIR", workspace_root.as_os_str()),
        (
            "CHATOS_PLUGIN_SANDBOX",
            OsString::from("windows-appcontainer-hook-v1").as_os_str(),
        ),
    ] {
        if names.insert(name.to_lowercase()) {
            environment.push((OsString::from(name), value.to_os_string()));
        }
    }
    environment
}
