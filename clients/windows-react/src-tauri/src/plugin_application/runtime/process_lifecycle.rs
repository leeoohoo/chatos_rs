use super::{
    loopback_proxy::{start_loopback_proxy, LoopbackProxyShutdown},
    process_preparation::{prepare_runtime_process, PreparedRuntimeProcess},
    static_server::spawn_static_runtime,
    ResolvedPluginApplication, ResolvedRuntimeContext, RunningPluginApplication,
    RuntimeTermination,
};
#[cfg(windows)]
use crate::windows_appcontainer::{WorkspaceAccess, WorkspaceLeaseManager};
use crate::{
    machine_resource_governor::MachineResourcePermit, terminal_runtime::process::ProcessTermination,
};
#[cfg(windows)]
use std::ffi::OsString;
use std::{
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StandardMutex,
    },
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::Notify,
    time::{timeout, Duration},
};

pub(super) async fn spawn_application_runtime(
    resolved: &ResolvedPluginApplication,
    context: &ResolvedRuntimeContext,
    resource_permit: Option<MachineResourcePermit>,
    #[cfg(windows)] sandbox: Option<Arc<WorkspaceLeaseManager>>,
) -> Result<(Arc<RunningPluginApplication>, u64), String> {
    if resolved.contribution.runtime.is_none() {
        return spawn_static_runtime(resolved)
            .await
            .map(|process| (process, 2_000));
    }
    let prepared = prepare_runtime_process(resolved, context)?;
    let launch_timeout = prepared.launch_timeout_ms;
    let process = spawn_runtime_process(
        resolved,
        prepared,
        resource_permit,
        #[cfg(windows)]
        sandbox,
    )
    .await?;
    Ok((process, launch_timeout))
}

async fn spawn_runtime_process(
    resolved: &ResolvedPluginApplication,
    prepared: PreparedRuntimeProcess,
    resource_permit: Option<MachineResourcePermit>,
    #[cfg(windows)] sandbox: Option<Arc<WorkspaceLeaseManager>>,
) -> Result<Arc<RunningPluginApplication>, String> {
    let proxy = start_loopback_proxy(
        prepared.proxy_port,
        prepared.child_port,
        prepared.health_path.clone(),
        prepared.authentication_token.clone(),
    )
    .await?;
    #[cfg(windows)]
    if let Some(sandbox) = sandbox {
        return spawn_sandboxed_runtime(resolved, prepared, resource_permit, sandbox, proxy).await;
    }
    let mut command = Command::new(&prepared.executable);
    command
        .args(&prepared.arguments)
        .current_dir(&prepared.installation_root)
        .env_clear()
        .envs(&prepared.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut child = command.spawn().map_err(|_| {
        proxy.request();
        "Plugin 应用后端启动失败。".to_owned()
    })?;
    let termination = match ProcessTermination::attach(&child) {
        Ok(termination) => termination,
        Err(_) => {
            proxy.request();
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err("无法隔离 Plugin 应用进程树。".into());
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let process = running_plugin_application(
        resolved,
        prepared,
        resource_permit,
        termination,
        proxy.clone(),
    );
    let stdout_task = stdout.map(|pipe| tokio::spawn(drain_pipe(pipe)));
    let stderr_task = stderr.map(|pipe| tokio::spawn(drain_pipe(pipe)));
    tokio::spawn(wait_for_process(
        process.clone(),
        child,
        stdout_task,
        stderr_task,
        proxy,
    ));
    Ok(process)
}

fn running_plugin_application(
    resolved: &ResolvedPluginApplication,
    prepared: PreparedRuntimeProcess,
    resource_permit: Option<MachineResourcePermit>,
    termination: ProcessTermination,
    proxy: Arc<LoopbackProxyShutdown>,
) -> Arc<RunningPluginApplication> {
    Arc::new(RunningPluginApplication {
        release_id: resolved.record.spec.release_id.clone(),
        release_digest: resolved.record.spec.release_digest.clone(),
        version: resolved.manifest.version.clone(),
        base_url: prepared.base_url,
        health_path: prepared.health_path,
        authentication_token: prepared.authentication_token,
        termination: RuntimeTermination::Process {
            process: termination,
            proxy,
        },
        alive: AtomicBool::new(true),
        finished: Notify::new(),
        resource_permit: StandardMutex::new(resource_permit),
    })
}

#[cfg(windows)]
async fn spawn_sandboxed_runtime(
    resolved: &ResolvedPluginApplication,
    prepared: PreparedRuntimeProcess,
    resource_permit: Option<MachineResourcePermit>,
    sandbox: Arc<WorkspaceLeaseManager>,
    proxy: Arc<LoopbackProxyShutdown>,
) -> Result<Arc<RunningPluginApplication>, String> {
    use std::sync::Mutex as StdMutex;

    let arguments = prepared
        .arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let environment = prepared
        .environment
        .iter()
        .map(|(key, value)| (OsString::from(key), OsString::from(value)))
        .collect::<Vec<_>>();
    let mut child = sandbox
        .spawn_workspace_command_with_leases(
            &prepared.installation_root,
            &prepared.executable,
            &arguments,
            &[
                (
                    prepared.installation_root.clone(),
                    WorkspaceAccess::ReadOnly,
                ),
                (prepared._data.clone(), WorkspaceAccess::ReadWrite),
                (prepared._cache.clone(), WorkspaceAccess::ReadWrite),
            ],
            prepared.network_policy,
            &environment,
        )
        .map_err(|error| {
            proxy.request();
            format!("Plugin 应用后端受限进程启动失败：{error}")
        })?;
    let stdout = match child.take_stdout().map(spawn_blocking_drain) {
        Some(stdout) => stdout,
        None => {
            proxy.request();
            let _ = child.restore();
            return Err("Plugin 应用后端 stdout 不可用。".to_owned());
        }
    };
    let stderr = match child.take_stderr().map(spawn_blocking_drain) {
        Some(stderr) => stderr,
        None => {
            proxy.request();
            let _ = child.restore();
            return Err("Plugin 应用后端 stderr 不可用。".to_owned());
        }
    };
    let child = Arc::new(StdMutex::new(Some(child)));
    let termination = ProcessTermination::attach_sandboxed(child.clone());
    let process = running_plugin_application(
        resolved,
        prepared,
        resource_permit,
        termination,
        proxy.clone(),
    );
    tokio::spawn(wait_for_sandboxed_process(
        process.clone(),
        child,
        stdout,
        stderr,
        proxy,
    ));
    Ok(process)
}

#[cfg(windows)]
fn spawn_blocking_drain(mut file: std::fs::File) -> tokio::task::JoinHandle<()> {
    use std::io::Read;

    tokio::task::spawn_blocking(move || {
        let mut buffer = [0_u8; 8 * 1_024];
        loop {
            match file.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
        }
    })
}

#[cfg(windows)]
async fn wait_for_sandboxed_process(
    process: Arc<RunningPluginApplication>,
    child: Arc<std::sync::Mutex<Option<crate::windows_appcontainer::SandboxedChild>>>,
    stdout: tokio::task::JoinHandle<()>,
    stderr: tokio::task::JoinHandle<()>,
    proxy: Arc<LoopbackProxyShutdown>,
) {
    let _ = tokio::task::spawn_blocking(move || loop {
        let finished = child.lock().ok().and_then(|guard| {
            guard
                .as_ref()
                .map(crate::windows_appcontainer::SandboxedChild::try_wait)
        });
        match finished {
            Some(Ok(Some(_))) => {
                let _ = child
                    .lock()
                    .ok()
                    .and_then(|mut guard| guard.take())
                    .map(crate::windows_appcontainer::SandboxedChild::restore);
                return;
            }
            Some(Ok(None)) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Some(Err(_)) | None => return,
        }
    })
    .await;
    proxy.request();
    process.alive.store(false, Ordering::SeqCst);
    process.release_resource();
    process.finished.notify_waiters();
    let _ = timeout(Duration::from_secs(1), stdout).await;
    let _ = timeout(Duration::from_secs(1), stderr).await;
}

async fn drain_pipe(mut pipe: impl AsyncRead + Unpin) {
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        match pipe.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}

async fn wait_for_process(
    process: Arc<RunningPluginApplication>,
    mut child: Child,
    stdout: Option<tokio::task::JoinHandle<()>>,
    stderr: Option<tokio::task::JoinHandle<()>>,
    proxy: Arc<LoopbackProxyShutdown>,
) {
    let _ = child.wait().await;
    proxy.request();
    process.alive.store(false, Ordering::SeqCst);
    process.release_resource();
    process.finished.notify_waiters();
    if let Some(task) = stdout {
        let _ = timeout(Duration::from_secs(1), task).await;
    }
    if let Some(task) = stderr {
        let _ = timeout(Duration::from_secs(1), task).await;
    }
}
