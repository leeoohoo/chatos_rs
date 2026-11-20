use super::{
    ManagedProjectRun, ManagedProjectRunState, ManagedStatus, ProjectChangeSignal, ProjectRunScope,
};
use crate::{
    machine_resource_governor::MachineResourcePermit,
    project_run::models::ResolvedProjectRunTarget, terminal_runtime::process::ProcessTermination,
};
use std::{
    process::Stdio,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::{Mutex, Notify},
    time::{timeout, Duration},
};
use uuid::Uuid;

pub(super) async fn spawn_project_run(
    scope: &ProjectRunScope,
    mut target: ResolvedProjectRunTarget,
    sequence: u64,
    changed: Arc<ProjectChangeSignal>,
    resource_permit: MachineResourcePermit,
) -> Result<Arc<ManagedProjectRun>, String> {
    let mut redactions = target
        .environment
        .values()
        .filter_map(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    redactions.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
    redactions.dedup();
    prepare_launch_environment(&mut target)?;
    let mut command = project_command(&target.executable, &target.arguments);
    command
        .current_dir(&target.cwd)
        .envs(&target.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "无法启动项目运行目标。".to_owned())?;
    let termination = match ProcessTermination::attach(&child) {
        Ok(termination) => termination,
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err("无法隔离项目进程树。".into());
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let initial_log = format!("$ {}\n", target.presentation.command).into_bytes();
    let process = Arc::new(ManagedProjectRun {
        id: Uuid::new_v4().to_string(),
        sequence,
        owner_user_id: scope.owner_user_id.clone(),
        project_id: scope.project_id.clone(),
        project_root: scope.project_root.clone(),
        target_id: target.presentation.id,
        name: target.presentation.label,
        cwd: target.presentation.cwd,
        redactions,
        termination,
        state: Mutex::new(ManagedProjectRunState {
            status: ManagedStatus::Running,
            stop_requested: false,
            log: initial_log,
            log_truncated: false,
            started_at_unix_ms: now_unix_ms(),
            exit_code: None,
        }),
        changed,
        finished: Notify::new(),
        resource_permit: Mutex::new(Some(resource_permit)),
    });
    let stdout_task = stdout.map(|stdout| tokio::spawn(read_pipe(process.clone(), stdout)));
    let stderr_task = stderr.map(|stderr| tokio::spawn(read_pipe(process.clone(), stderr)));
    tokio::spawn(wait_for_child(
        process.clone(),
        child,
        stdout_task,
        stderr_task,
    ));
    Ok(process)
}

fn project_command(executable: &std::path::Path, arguments: &[String]) -> Command {
    #[cfg(windows)]
    if is_batch_script(executable) {
        let mut command = Command::new(
            std::env::var_os("ComSpec")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "cmd.exe".into()),
        );
        let mut command_line = quote_windows_command_arg(executable.as_os_str());
        for argument in arguments {
            command_line.push(' ');
            command_line.push_str(&quote_windows_command_arg(std::ffi::OsStr::new(argument)));
        }
        // `/c` consumes the remainder of the command line.  Passing that
        // remainder through `arg` makes Rust escape the embedded quotes for
        // CreateProcess, so cmd.exe receives `\"C:\\Program Files...\"` as
        // the command name.  Keep the command line raw after `/c` so quoted
        // batch paths remain executable when the path contains spaces.
        command.arg("/d").arg("/s").arg("/c");
        std::os::windows::process::CommandExt::raw_arg(
            command.as_std_mut(),
            // The outer pair is consumed by cmd's `/s /c` parser; the
            // inner pair stays around the batch path itself.
            format!(" \"{command_line}\""),
        );
        return command;
    }

    let mut command = Command::new(executable);
    command.args(arguments);
    command
}

#[cfg(windows)]
fn is_batch_script(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "cmd" | "bat"))
}

#[cfg(windows)]
fn quote_windows_command_arg(value: &std::ffi::OsStr) -> String {
    let value = value.to_string_lossy();
    if value.is_empty() {
        return "\"\"".into();
    }
    let needs_quotes = value
        .chars()
        .any(|character| character.is_whitespace() || character == '\"');
    if !needs_quotes {
        return value.into_owned();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for character in value.chars() {
        match character {
            '\\' => backslashes += 1,
            '\"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('\"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(character);
                backslashes = 0;
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('\"');
    quoted
}

fn prepare_launch_environment(target: &mut ResolvedProjectRunTarget) -> Result<(), String> {
    if target.toolchain_directories.is_empty() {
        return Ok(());
    }
    let path_key = target
        .environment
        .keys()
        .find(|key| key.to_string_lossy().eq_ignore_ascii_case("PATH"))
        .cloned();
    let base = path_key
        .as_ref()
        .and_then(|key| target.environment.remove(key))
        .or_else(|| std::env::var_os("PATH"));
    let mut entries = target.toolchain_directories.clone();
    if let Some(base) = base {
        entries.extend(std::env::split_paths(&base));
    }
    let path = std::env::join_paths(entries).map_err(|_| "无法构建项目工具链环境。".to_owned())?;
    target.environment.insert("PATH".into(), path);
    Ok(())
}

async fn read_pipe(process: Arc<ManagedProjectRun>, mut pipe: impl AsyncRead + Unpin) {
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        match pipe.read(&mut buffer).await {
            Ok(0) => return,
            Ok(count) => process.append(&buffer[..count]).await,
            Err(_) => {
                process.append("\n[读取项目输出失败]\n".as_bytes()).await;
                return;
            }
        }
    }
}

async fn wait_for_child(
    process: Arc<ManagedProjectRun>,
    mut child: Child,
    stdout_task: Option<tokio::task::JoinHandle<()>>,
    stderr_task: Option<tokio::task::JoinHandle<()>>,
) {
    let exit_code = child.wait().await.ok().and_then(|status| status.code());
    let _ = process.termination.terminate(true);
    if let Some(task) = stdout_task {
        let _ = timeout(Duration::from_secs(1), task).await;
    }
    if let Some(task) = stderr_task {
        let _ = timeout(Duration::from_secs(1), task).await;
    }
    process
        .append(format!("\n[进程已退出，代码 {}]\n", exit_code.unwrap_or(-1)).as_bytes())
        .await;
    process.finish(exit_code).await;
}

pub(super) async fn terminate_all(processes: Vec<Arc<ManagedProjectRun>>, reason: &str) {
    for process in &processes {
        process.terminate_now();
    }
    for process in processes {
        process.changed.bump();
        let _ = process.stop(reason).await;
    }
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
