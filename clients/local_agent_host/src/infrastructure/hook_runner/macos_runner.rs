#[derive(Debug, Clone)]
pub struct MacOsSeatbeltHookRunner {
    #[cfg(target_os = "macos")]
    data_root: PathBuf,
}

impl MacOsSeatbeltHookRunner {
    pub fn new(data_root: PathBuf) -> Result<Self, String> {
        if !data_root.is_absolute() {
            return Err("Hook data root must be absolute".to_string());
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Self { data_root })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = data_root;
            Ok(Self {})
        }
    }

    #[cfg(target_os = "macos")]
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

#[async_trait]
impl LocalHookRunner for MacOsSeatbeltHookRunner {
    async fn run(
        &self,
        invocation: LocalHookInvocation,
    ) -> Result<LocalHookInvocationResult, String> {
        invocation.bundle.validate()?;
        if !invocation.bundle.hooks.contains(&invocation.hook) {
            return Err("Hook invocation is not part of its trusted bundle".to_string());
        }
        if invocation.stdin_json.len() > 256 * 1_024 {
            return Err("Hook stdin exceeds 262144 bytes".to_string());
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = invocation;
            return Err(
                "macOS Seatbelt Hook execution is unavailable on this platform".to_string(),
            );
        }
        #[cfg(target_os = "macos")]
        self.run_macos(invocation).await
    }
}

#[cfg(target_os = "macos")]
impl MacOsSeatbeltHookRunner {
    async fn run_macos(
        &self,
        invocation: LocalHookInvocation,
    ) -> Result<LocalHookInvocationResult, String> {
        use std::os::unix::{fs::PermissionsExt, process::CommandExt};

        let installation_root = canonical_directory(
            Path::new(&invocation.bundle.installation_root),
            "Hook installation root",
        )?;
        let workspace_root = invocation
            .workspace_root
            .as_deref()
            .map(|path| canonical_directory(path, "Hook workspace root"))
            .transpose()?;
        let configured_data_root = self.plugin_data_path(&invocation.bundle);
        std::fs::create_dir_all(&configured_data_root)
            .map_err(|error| format!("create Hook data directory failed: {error}"))?;
        std::fs::set_permissions(
            &configured_data_root,
            std::fs::Permissions::from_mode(0o700),
        )
        .map_err(|error| format!("protect Hook data directory failed: {error}"))?;
        let data_root = canonical_directory(&configured_data_root, "Hook data root")?;
        let temporary_root = data_root.join("tmp");
        std::fs::create_dir_all(&temporary_root)
            .map_err(|error| format!("create Hook temporary directory failed: {error}"))?;

        let profile = seatbelt_profile(
            &installation_root,
            &data_root,
            workspace_root.as_deref(),
            &invocation.bundle.permissions,
        );
        let environment = hook_environment(
            &installation_root,
            &data_root,
            &temporary_root,
            workspace_root.as_deref(),
        );
        let mut command = Command::new(SANDBOX_EXECUTABLE);
        command
            .arg("-p")
            .arg(profile)
            .arg(SHELL_EXECUTABLE)
            .arg("-c")
            .arg(&invocation.hook.command)
            .current_dir(workspace_root.as_deref().unwrap_or(&installation_root))
            .env_clear()
            .envs(environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command.as_std_mut().process_group(0);
        let mut child = command
            .spawn()
            .map_err(|error| format!("start sandboxed Hook failed: {error}"))?;
        let process_id = child
            .id()
            .ok_or_else(|| "sandboxed Hook has no process id".to_string())?;
        let mut group_guard = ProcessGroupGuard::new(process_id);
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "sandboxed Hook has no stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "sandboxed Hook has no stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "sandboxed Hook has no stderr".to_string())?;
        let stdin_json = invocation.stdin_json;
        let stdin_task = tokio::spawn(async move {
            let result = stdin.write_all(&stdin_json).await;
            drop(stdin);
            result
        });
        let stdout_task = tokio::spawn(read_bounded(stdout, MAXIMUM_STDOUT_BYTES));
        let stderr_task = tokio::spawn(read_bounded(stderr, MAXIMUM_STDERR_BYTES));
        let started = Instant::now();
        let deadline = Duration::from_millis(invocation.hook.timeout_ms);
        let (status, timed_out) = match timeout(deadline, child.wait()).await {
            Ok(result) => (
                result.map_err(|error| format!("wait for sandboxed Hook failed: {error}"))?,
                false,
            ),
            Err(_) => {
                terminate_process_group(process_id);
                let status = match timeout(TERMINATION_GRACE, child.wait()).await {
                    Ok(result) => {
                        result.map_err(|error| format!("reap timed out Hook failed: {error}"))?
                    }
                    Err(_) => {
                        kill_process_group(process_id);
                        child
                            .wait()
                            .await
                            .map_err(|error| format!("reap killed Hook failed: {error}"))?
                    }
                };
                (status, true)
            }
        };
        group_guard.disarm();
        let _ = stdin_task.await;
        let (stdout, stdout_overflow) = stdout_task
            .await
            .map_err(|error| format!("join Hook stdout reader failed: {error}"))??;
        let (stderr, stderr_overflow) = stderr_task
            .await
            .map_err(|error| format!("join Hook stderr reader failed: {error}"))??;
        Ok(LocalHookInvocationResult {
            exit_code: status.code(),
            stdout,
            stderr,
            stdout_overflow,
            stderr_overflow,
            timed_out,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }
}
