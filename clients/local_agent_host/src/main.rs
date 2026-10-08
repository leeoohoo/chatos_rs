// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::AiRuntime;
use chatos_local_agent_host::{
    application::LocalAgentRuntime, infrastructure::SqliteClientStorage, serve_reader_writer,
    ChildEnvironmentModelCredentialResolver, LocalAgentHostAssembly, LocalAgentHostCoordinator,
    LocalControlPlaneSnapshot, LocalMemoryRuntimeConfig,
};
use std::{env, error::Error, future::Future, io, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::watch;

const COORDINATOR_SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

enum IpcMode {
    Stdio,
    #[cfg(unix)]
    UnixSocket(PathBuf),
    #[cfg(windows)]
    NamedPipe(String),
}

struct Options {
    database: PathBuf,
    owner_user_id: String,
    mode: IpcMode,
    workers_enabled: bool,
    read_only_tools: Vec<String>,
    approval_exempt_tools: Vec<String>,
    memory: Option<MemoryOptions>,
}

struct MemoryOptions {
    base_url: String,
    source_id: String,
    timeout_ms: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let options = match parse_options(env::args().skip(1).collect()) {
        Ok(options) => options,
        Err(message) if message == "help" => {
            print_help();
            return Ok(());
        }
        Err(message) => {
            eprintln!("{message}");
            print_help();
            std::process::exit(2);
        }
    };
    let storage = Arc::new(SqliteClientStorage::connect_file(&options.database).await?);
    let runtime = Arc::new(LocalAgentRuntime::new(storage.clone()));
    let recovered = runtime.initialize(&options.owner_user_id).await?;
    if recovered > 0 {
        eprintln!("Local Agent Host recovered {recovered} expired claim(s) for the active owner");
    }
    let (runner, memory_services) = match options.memory.as_ref() {
        Some(memory) => {
            let config = LocalMemoryRuntimeConfig::new(
                memory.base_url.clone(),
                memory.source_id.clone(),
                Duration::from_millis(memory.timeout_ms),
            )
            .with_access_token(non_empty_env("CHATOS_MEMORY_ACCESS_TOKEN"));
            let services = config.build_services(storage.clone(), options.owner_user_id.clone())?;
            (
                Arc::new(services.runner),
                Some((services.source_id, services.sync_worker)),
            )
        }
        None => (
            Arc::new(AiRuntime::builder().build_contextual_turn_runner()),
            None,
        ),
    };
    let control_plane = Arc::new(
        LocalControlPlaneSnapshot::new()
            .with_capability_store(storage.clone())
            .with_model_store(
                storage,
                runner,
                Arc::new(ChildEnvironmentModelCredentialResolver),
            ),
    );
    let assembly = match memory_services {
        Some((source_id, sync_worker)) => {
            LocalAgentHostAssembly::with_external_tool_worker_and_memory(
                runtime,
                options.owner_user_id,
                control_plane.clone(),
                control_plane,
                options.read_only_tools,
                options.approval_exempt_tools,
                source_id,
                sync_worker,
            )
        }
        None => LocalAgentHostAssembly::with_external_tool_worker(
            runtime,
            options.owner_user_id,
            control_plane.clone(),
            control_plane,
            options.read_only_tools,
            options.approval_exempt_tools,
        ),
    }
    .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    let coordinator = assembly.coordinator();
    match options.mode {
        IpcMode::Stdio => {
            serve_with_coordinator(
                coordinator,
                serve_stdio_until_parent_exit(assembly.coordinator()),
                options.workers_enabled,
            )
            .await?;
        }
        #[cfg(unix)]
        IpcMode::UnixSocket(path) => {
            serve_with_coordinator(
                coordinator,
                chatos_local_agent_host::unix::serve(&path, assembly.coordinator()),
                options.workers_enabled,
            )
            .await?;
        }
        #[cfg(windows)]
        IpcMode::NamedPipe(name) => {
            serve_with_coordinator(
                coordinator,
                chatos_local_agent_host::windows::serve(&name, assembly.coordinator()),
                options.workers_enabled,
            )
            .await?;
        }
    }
    Ok(())
}

#[cfg(unix)]
async fn serve_stdio_until_parent_exit(
    handler: Arc<LocalAgentHostCoordinator>,
) -> Result<(), chatos_local_agent_host::HostTransportError> {
    let parent_pid = unsafe { libc::getppid() };
    tokio::select! {
        result = serve_reader_writer(tokio::io::stdin(), tokio::io::stdout(), handler) => result,
        _ = wait_for_parent_exit(parent_pid) => Ok(()),
    }
}

#[cfg(not(unix))]
async fn serve_stdio_until_parent_exit(
    handler: Arc<LocalAgentHostCoordinator>,
) -> Result<(), chatos_local_agent_host::HostTransportError> {
    serve_reader_writer(tokio::io::stdin(), tokio::io::stdout(), handler).await
}

#[cfg(unix)]
async fn wait_for_parent_exit(parent_pid: libc::pid_t) {
    loop {
        let current_parent_pid = unsafe { libc::getppid() };
        if parent_process_changed(parent_pid, current_parent_pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[cfg(unix)]
fn parent_process_changed(expected: libc::pid_t, current: libc::pid_t) -> bool {
    expected <= 1 || current != expected
}

fn parse_options(arguments: Vec<String>) -> Result<Options, String> {
    let mut database = None;
    let mut owner_user_id = None;
    let mut mode = None;
    let mut workers_enabled = true;
    let mut read_only_tools = Vec::new();
    let mut approval_exempt_tools = Vec::new();
    let mut memory_base_url = None;
    let mut memory_source_id = None;
    let mut memory_timeout_ms = 30_000;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--help" | "-h" => return Err("help".to_string()),
            "--database" => {
                index += 1;
                database = Some(required_value(&arguments, index, "--database")?.into());
            }
            "--owner-user-id" => {
                index += 1;
                owner_user_id = Some(required_value(&arguments, index, "--owner-user-id")?.into());
            }
            "--read-only-tool" => {
                index += 1;
                read_only_tools.push(required_value(&arguments, index, "--read-only-tool")?.into());
            }
            "--approval-exempt-tool" => {
                index += 1;
                approval_exempt_tools
                    .push(required_value(&arguments, index, "--approval-exempt-tool")?.into());
            }
            "--memory-base-url" => {
                index += 1;
                memory_base_url =
                    Some(required_value(&arguments, index, "--memory-base-url")?.to_string());
            }
            "--memory-source-id" => {
                index += 1;
                memory_source_id =
                    Some(required_value(&arguments, index, "--memory-source-id")?.to_string());
            }
            "--memory-timeout-ms" => {
                index += 1;
                let value = required_value(&arguments, index, "--memory-timeout-ms")?;
                memory_timeout_ms = value
                    .parse::<u64>()
                    .map_err(|_| "--memory-timeout-ms must be an integer".to_string())?;
                if !(1..=300_000).contains(&memory_timeout_ms) {
                    return Err("--memory-timeout-ms must be between 1 and 300000".to_string());
                }
            }
            "--disable-workers" => workers_enabled = false,
            "--stdio" => set_mode(&mut mode, IpcMode::Stdio)?,
            #[cfg(unix)]
            "--socket" => {
                index += 1;
                let path = required_value(&arguments, index, "--socket")?;
                set_mode(&mut mode, IpcMode::UnixSocket(path.into()))?;
            }
            #[cfg(windows)]
            "--pipe" => {
                index += 1;
                let name = required_value(&arguments, index, "--pipe")?;
                set_mode(&mut mode, IpcMode::NamedPipe(name.to_string()))?;
            }
            value => return Err(format!("unknown argument: {value}")),
        }
        index += 1;
    }
    let memory = match (memory_base_url, memory_source_id) {
        (Some(base_url), Some(source_id)) => Some(MemoryOptions {
            base_url,
            source_id,
            timeout_ms: memory_timeout_ms,
        }),
        (None, None) => None,
        _ => {
            return Err(
                "--memory-base-url and --memory-source-id must be provided together".to_string(),
            )
        }
    };
    Ok(Options {
        database: database.ok_or_else(|| "--database is required".to_string())?,
        owner_user_id: owner_user_id.ok_or_else(|| "--owner-user-id is required".to_string())?,
        mode: mode.ok_or_else(|| "one IPC mode is required".to_string())?,
        workers_enabled,
        read_only_tools,
        approval_exempt_tools,
        memory,
    })
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn required_value<'a>(
    arguments: &'a [String],
    index: usize,
    option: &str,
) -> Result<&'a str, String> {
    arguments
        .get(index)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{option} requires a value"))
}

fn set_mode(target: &mut Option<IpcMode>, value: IpcMode) -> Result<(), String> {
    if target.is_some() {
        return Err("only one IPC mode may be selected".to_string());
    }
    *target = Some(value);
    Ok(())
}

fn print_help() {
    eprintln!("ChatOS Local Agent Host");
    eprintln!("  --database <path>   Client-owned SQLite database");
    eprintln!("  --owner-user-id <id>  Scope IPC and workers to the signed-in account");
    eprintln!("  --read-only-tool <name>  Mark a native tool as replay-safe; repeat as needed");
    eprintln!("  --approval-exempt-tool <name>  Keep a side-effecting tool claimable without first-execution approval; repeat as needed");
    eprintln!("  --memory-base-url <url>  Enable retained Memory compose and record sync");
    eprintln!("  --memory-source-id <id>  Memory source paired with --memory-base-url");
    eprintln!("  --memory-timeout-ms <ms>  Memory request timeout (default: 30000)");
    eprintln!(
        "  --disable-workers    Serve IPC without claiming model or tool work during bootstrap"
    );
    eprintln!("  --stdio             Serve framed JSON on stdin/stdout");
    #[cfg(unix)]
    eprintln!("  --socket <path>     Serve a permission-restricted Unix socket");
    #[cfg(windows)]
    eprintln!(r"  --pipe <name>       Serve \\.\pipe\chatos-local-agent-*");
}

async fn serve_with_coordinator<F>(
    coordinator: Arc<LocalAgentHostCoordinator>,
    serve: F,
    workers_enabled: bool,
) -> Result<(), Box<dyn Error>>
where
    F: Future<Output = Result<(), chatos_local_agent_host::HostTransportError>>,
{
    if !workers_enabled {
        serve.await?;
        return Ok(());
    }
    let (shutdown, receiver) = watch::channel(false);
    let mut coordinator_task = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.run_until_shutdown(receiver).await }
    });
    tokio::select! {
        serve_result = serve => {
            let _ = shutdown.send(true);
            await_coordinator_shutdown(coordinator_task, COORDINATOR_SHUTDOWN_GRACE).await?;
            serve_result?;
            Ok(())
        }
        coordinator_result = &mut coordinator_task => {
            let _ = shutdown.send(true);
            coordinator_result??;
            Err(io::Error::other("Local Agent coordinator stopped unexpectedly").into())
        }
    }
}

async fn await_coordinator_shutdown(
    mut coordinator_task: tokio::task::JoinHandle<
        Result<(), chatos_local_agent_host::LocalAgentCoordinatorError>,
    >,
    grace: Duration,
) -> Result<(), Box<dyn Error>> {
    match tokio::time::timeout(grace, &mut coordinator_task).await {
        Ok(result) => result??,
        Err(_) => {
            coordinator_task.abort();
            match coordinator_task.await {
                Err(error) if error.is_cancelled() => {}
                Err(error) => return Err(error.into()),
                Ok(result) => result?,
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[cfg(unix)]
    #[test]
    fn stdio_parent_watchdog_detects_reparenting() {
        assert!(!parent_process_changed(42, 42));
        assert!(parent_process_changed(42, 1));
        assert!(parent_process_changed(1, 1));
    }

    #[test]
    fn parses_bootstrap_mode_without_workers() {
        let options = parse_options(vec![
            "--database".to_string(),
            "/tmp/local-agent.sqlite3".to_string(),
            "--owner-user-id".to_string(),
            "user-1".to_string(),
            "--disable-workers".to_string(),
            "--stdio".to_string(),
        ])
        .expect("bootstrap options");

        assert!(!options.workers_enabled);
    }

    #[tokio::test]
    async fn coordinator_shutdown_aborts_work_that_exceeds_the_grace_period() {
        struct DropProbe(Arc<AtomicBool>);
        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let coordinator_task = tokio::spawn({
            let dropped = Arc::clone(&dropped);
            async move {
                let _probe = DropProbe(dropped);
                std::future::pending::<()>().await;
                Ok::<(), chatos_local_agent_host::LocalAgentCoordinatorError>(())
            }
        });
        tokio::task::yield_now().await;
        await_coordinator_shutdown(coordinator_task, Duration::from_millis(10))
            .await
            .expect("bounded shutdown");
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[test]
    fn parses_repeated_read_only_tools_without_accepting_secret_arguments() {
        let options = parse_options(
            [
                "--database",
                "/tmp/local-agent.sqlite",
                "--owner-user-id",
                "user-1",
                "--read-only-tool",
                "read_file",
                "--read-only-tool",
                "list_files",
                "--approval-exempt-tool",
                "stage_edit_batch",
                "--stdio",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        )
        .expect("options");
        assert_eq!(
            options.read_only_tools,
            vec!["read_file".to_string(), "list_files".to_string()]
        );
        assert_eq!(options.owner_user_id, "user-1");
        assert_eq!(
            options.approval_exempt_tools,
            vec!["stage_edit_batch".to_string()]
        );
        assert!(matches!(options.mode, IpcMode::Stdio));
        assert!(options.memory.is_none());
        assert!(parse_options(vec!["--api-key".to_string(), "secret".to_string()]).is_err());
        assert!(parse_options(vec![
            "--memory-access-token".to_string(),
            "secret".to_string()
        ])
        .is_err());
        assert!(parse_options(
            ["--database", "/tmp/local-agent.sqlite", "--stdio"]
                .into_iter()
                .map(str::to_string)
                .collect()
        )
        .is_err());
    }

    #[test]
    fn parses_complete_optional_memory_configuration() {
        let options = parse_options(
            [
                "--database",
                "/tmp/local-agent.sqlite",
                "--owner-user-id",
                "user-1",
                "--memory-base-url",
                "https://memory.example.test",
                "--memory-source-id",
                "local_agent",
                "--memory-timeout-ms",
                "12000",
                "--stdio",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        )
        .expect("options");
        let memory = options.memory.expect("memory");
        assert_eq!(memory.source_id, "local_agent");
        assert_eq!(memory.timeout_ms, 12_000);

        let incomplete = parse_options(
            [
                "--database",
                "/tmp/local-agent.sqlite",
                "--owner-user-id",
                "user-1",
                "--memory-base-url",
                "https://memory.example.test",
                "--stdio",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        );
        assert!(incomplete.is_err());
    }
}
