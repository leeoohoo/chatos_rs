use chrono::Utc;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub error: Option<String>,
    pub timed_out: bool,
}

#[derive(Copy, Clone)]
enum StreamKind {
    Stdout,
    Stderr,
}

enum StreamEvent {
    Data(StreamKind, Vec<u8>),
    Done(StreamKind),
}

pub fn run_command(
    exec: &[String],
    env: &HashMap<String, String>,
    cwd: Option<&str>,
    timeout_ms: i64,
    max_output_bytes: usize,
    input: Option<&str>,
    cancel_flag: Option<&AtomicBool>,
) -> Result<RunResult, String> {
    if exec.is_empty() {
        return Err("command is required".to_string());
    }

    let command = &exec[0];
    let args = &exec[1..];

    let mut cmd = Command::new(command);
    cmd.args(args);
    if let Some(dir) = cwd {
        if !dir.trim().is_empty() {
            cmd.current_dir(dir);
        }
    }
    cmd.envs(env);
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|err| err.to_string())?;
    if let Some(text) = input {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|err| err.to_string())?;
        }
    }

    let started_at = Utc::now().to_rfc3339();
    let start_time = Instant::now();

    let stdout = child.stdout.take().ok_or("failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("failed to capture stderr")?;

    let (tx, rx) = mpsc::channel();
    spawn_reader(stdout, tx.clone(), StreamKind::Stdout);
    spawn_reader(stderr, tx.clone(), StreamKind::Stderr);

    let mut stdout_text = String::new();
    let mut stderr_text = String::new();
    let mut stdout_truncated = false;
    let mut stderr_truncated = false;
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut exit_status: Option<std::process::ExitStatus> = None;
    let mut timed_out = false;
    let mut cancelled = false;

    loop {
        if exit_status.is_none() {
            if let Ok(Some(status)) = child.try_wait() {
                exit_status = Some(status);
            }
        }

        if stdout_done && stderr_done && exit_status.is_some() {
            break;
        }

        if timeout_ms > 0 && !timed_out && start_time.elapsed().as_millis() as i64 >= timeout_ms {
            timed_out = true;
            let _ = child.kill();
        }

        if let Some(flag) = cancel_flag {
            if flag.load(Ordering::Relaxed) && !cancelled {
                cancelled = true;
                let _ = child.kill();
            }
        }

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(StreamEvent::Data(kind, chunk)) => match kind {
                StreamKind::Stdout => {
                    if !stdout_truncated {
                        let remaining = max_output_bytes.saturating_sub(stdout_text.len());
                        let slice = if chunk.len() > remaining {
                            stdout_truncated = true;
                            &chunk[..remaining]
                        } else {
                            &chunk[..]
                        };
                        stdout_text.push_str(&String::from_utf8_lossy(slice));
                    }
                }
                StreamKind::Stderr => {
                    if !stderr_truncated {
                        let remaining = max_output_bytes.saturating_sub(stderr_text.len());
                        let slice = if chunk.len() > remaining {
                            stderr_truncated = true;
                            &chunk[..remaining]
                        } else {
                            &chunk[..]
                        };
                        stderr_text.push_str(&String::from_utf8_lossy(slice));
                    }
                }
            },
            Ok(StreamEvent::Done(kind)) => match kind {
                StreamKind::Stdout => stdout_done = true,
                StreamKind::Stderr => stderr_done = true,
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
    }

    let status = match exit_status {
        Some(value) => value,
        None => child.wait().map_err(|err| err.to_string())?,
    };

    let finished_at = Utc::now().to_rfc3339();
    let duration_ms = start_time.elapsed().as_millis() as i64;
    let exit_code = status.code();

    if stdout_text.is_empty() {
        stdout_text = "(empty)".to_string();
    }
    if stderr_text.is_empty() {
        stderr_text = "(empty)".to_string();
    }

    Ok(RunResult {
        stdout: stdout_text,
        stderr: stderr_text,
        exit_code,
        signal: None,
        started_at,
        finished_at,
        duration_ms,
        stdout_truncated,
        stderr_truncated,
        error: if cancelled {
            Some("cancelled".to_string())
        } else {
            None
        },
        timed_out,
    })
}

fn spawn_reader(
    mut reader: impl Read + Send + 'static,
    tx: mpsc::Sender<StreamEvent>,
    kind: StreamKind,
) {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    let _ = tx.send(StreamEvent::Done(kind));
                    break;
                }
                Ok(n) => {
                    let _ = tx.send(StreamEvent::Data(kind, buffer[..n].to_vec()));
                }
                Err(_) => {
                    let _ = tx.send(StreamEvent::Done(kind));
                    break;
                }
            }
        }
    });
}
