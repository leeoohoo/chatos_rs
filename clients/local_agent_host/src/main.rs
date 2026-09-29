// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_host::{
    application::LocalAgentRuntime, infrastructure::SqliteClientStorage, serve_reader_writer,
};
use std::{env, error::Error, path::PathBuf, sync::Arc};

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
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    let recovered = runtime.initialize().await?;
    if recovered > 0 {
        eprintln!("Local Agent Host moved {recovered} expired claim(s) to needs_review");
    }
    match options.mode {
        IpcMode::Stdio => {
            let mut input = tokio::io::stdin();
            let mut output = tokio::io::stdout();
            serve_reader_writer(&mut input, &mut output, runtime).await?;
        }
        #[cfg(unix)]
        IpcMode::UnixSocket(path) => {
            chatos_local_agent_host::unix::serve(&path, runtime).await?;
        }
        #[cfg(windows)]
        IpcMode::NamedPipe(name) => {
            chatos_local_agent_host::windows::serve(&name, runtime).await?;
        }
    }
    Ok(())
}

fn parse_options(arguments: Vec<String>) -> Result<Options, String> {
    let mut database = None;
    let mut mode = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--help" | "-h" => return Err("help".to_string()),
            "--database" => {
                index += 1;
                database = Some(required_value(&arguments, index, "--database")?.into());
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
    eprintln!("  --stdio             Serve framed JSON on stdin/stdout");
    #[cfg(unix)]
    eprintln!("  --socket <path>     Serve a permission-restricted Unix socket");
    #[cfg(windows)]
    eprintln!(r"  --pipe <name>       Serve \\.\pipe\chatos-local-agent-*");
}
