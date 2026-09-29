// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::AiRuntime;
use chatos_local_agent_host::{
    application::LocalAgentRuntime, infrastructure::SqliteClientStorage, serve_reader_writer,
    ChildEnvironmentModelCredentialResolver, LocalAgentHostAssembly, LocalAgentHostCoordinator,
    LocalControlPlaneSnapshot,
};
use std::{env, error::Error, future::Future, io, path::PathBuf, sync::Arc};
use tokio::sync::watch;

enum IpcMode {
    Stdio,
    #[cfg(unix)]
    UnixSocket(PathBuf),
    #[cfg(windows)]
    NamedPipe(String),
}

struct Options {
    database: PathBuf,
    mode: IpcMode,
    read_only_tools: Vec<String>,
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
    let recovered = runtime.initialize().await?;
    if recovered > 0 {
        eprintln!("Local Agent Host moved {recovered} expired claim(s) to needs_review");
    }
    let runner = Arc::new(AiRuntime::builder().build_contextual_turn_runner());
    let control_plane = Arc::new(
        LocalControlPlaneSnapshot::new()
            .with_capability_store(storage.clone())
            .with_model_store(
                storage,
                runner,
                Arc::new(ChildEnvironmentModelCredentialResolver),
            ),
    );
    let assembly = LocalAgentHostAssembly::with_external_tool_worker(
        runtime,
        control_plane.clone(),
        control_plane,
        options.read_only_tools,
    )
    .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    let coordinator = assembly.coordinator();
    match options.mode {
        IpcMode::Stdio => {
            let mut input = tokio::io::stdin();
            let mut output = tokio::io::stdout();
            serve_with_coordinator(
                coordinator,
                serve_reader_writer(&mut input, &mut output, assembly.coordinator()),
            )
            .await?;
        }
        #[cfg(unix)]
        IpcMode::UnixSocket(path) => {
            serve_with_coordinator(
                coordinator,
                chatos_local_agent_host::unix::serve(&path, assembly.coordinator()),
            )
            .await?;
        }
        #[cfg(windows)]
        IpcMode::NamedPipe(name) => {
            serve_with_coordinator(
                coordinator,
                chatos_local_agent_host::windows::serve(&name, assembly.coordinator()),
            )
            .await?;
        }
    }
    Ok(())
}

fn parse_options(arguments: Vec<String>) -> Result<Options, String> {
    let mut database = None;
    let mut mode = None;
    let mut read_only_tools = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--help" | "-h" => return Err("help".to_string()),
            "--database" => {
                index += 1;
                database = Some(required_value(&arguments, index, "--database")?.into());
            }
            "--read-only-tool" => {
                index += 1;
                read_only_tools.push(required_value(&arguments, index, "--read-only-tool")?.into());
            }
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
    Ok(Options {
        database: database.ok_or_else(|| "--database is required".to_string())?,
        mode: mode.ok_or_else(|| "one IPC mode is required".to_string())?,
        read_only_tools,
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
    eprintln!("  --read-only-tool <name>  Mark a native tool as replay-safe; repeat as needed");
    eprintln!("  --stdio             Serve framed JSON on stdin/stdout");
    #[cfg(unix)]
    eprintln!("  --socket <path>     Serve a permission-restricted Unix socket");
    #[cfg(windows)]
    eprintln!(r"  --pipe <name>       Serve \\.\pipe\chatos-local-agent-*");
}

async fn serve_with_coordinator<F>(
    coordinator: Arc<LocalAgentHostCoordinator>,
    serve: F,
) -> Result<(), Box<dyn Error>>
where
    F: Future<Output = Result<(), chatos_local_agent_host::HostTransportError>>,
{
    let (shutdown, receiver) = watch::channel(false);
    let mut coordinator_task = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.run_until_shutdown(receiver).await }
    });
    tokio::select! {
        serve_result = serve => {
            let _ = shutdown.send(true);
            coordinator_task.await??;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repeated_read_only_tools_without_accepting_secret_arguments() {
        let options = parse_options(
            [
                "--database",
                "/tmp/local-agent.sqlite",
                "--read-only-tool",
                "read_file",
                "--read-only-tool",
                "list_files",
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
        assert!(matches!(options.mode, IpcMode::Stdio));
        assert!(parse_options(vec!["--api-key".to_string(), "secret".to_string()]).is_err());
    }
}
