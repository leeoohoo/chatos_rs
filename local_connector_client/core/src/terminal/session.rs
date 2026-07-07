// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{anyhow, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use crate::relay::terminal_event;
use crate::select_local_shell;

mod input;
mod output;

#[derive(Debug)]
pub(crate) struct InteractiveCommandSubmission {
    pub(crate) command: String,
    pub(crate) cwd: PathBuf,
    pub(crate) blocked_reason: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct LocalTerminalManager {
    sessions: Arc<Mutex<BTreeMap<String, Arc<LocalPtySession>>>>,
}

pub(crate) struct LocalPtySession {
    id: String,
    root_cwd: PathBuf,
    current_cwd: StdMutex<PathBuf>,
    input_line: StdMutex<String>,
    writer: StdMutex<Box<dyn Write + Send>>,
    master: StdMutex<Box<dyn MasterPty + Send>>,
    child_killer: StdMutex<Box<dyn ChildKiller + Send + Sync>>,
    outbound: mpsc::UnboundedSender<Value>,
    output_history: StdMutex<String>,
    busy: AtomicBool,
    exited: AtomicBool,
}

impl LocalTerminalManager {
    pub(crate) async fn ensure_session(
        &self,
        session_id: String,
        root_cwd: PathBuf,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        outbound: mpsc::UnboundedSender<Value>,
    ) -> Result<Arc<LocalPtySession>> {
        {
            let sessions = self.sessions.lock().await;
            if let Some(existing) = sessions.get(session_id.as_str()) {
                if !existing.exited.load(Ordering::SeqCst) {
                    let _ = existing.resize(cols, rows);
                    return Ok(existing.clone());
                }
            }
        }

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| anyhow!("open pty failed: {err}"))?;
        let shell = select_local_shell();
        let mut command = CommandBuilder::new(shell);
        command.cwd(cwd.as_path());
        command.env("PWD", cwd.display().to_string());
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| anyhow!("spawn shell failed: {err}"))?;
        let child_killer = child.clone_killer();
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| anyhow!("clone pty reader failed: {err}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|err| anyhow!("take pty writer failed: {err}"))?;

        let session = Arc::new(LocalPtySession {
            id: session_id.clone(),
            root_cwd,
            current_cwd: StdMutex::new(cwd.clone()),
            input_line: StdMutex::new(String::new()),
            writer: StdMutex::new(writer),
            master: StdMutex::new(pair.master),
            child_killer: StdMutex::new(child_killer),
            outbound: outbound.clone(),
            output_history: StdMutex::new(String::new()),
            busy: AtomicBool::new(false),
            exited: AtomicBool::new(false),
        });

        let read_session = session.clone();
        let read_outbound = outbound.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        read_session.append_output(data.as_str());
                        let _ = read_outbound.send(terminal_event(
                            "terminal_output",
                            read_session.id.as_str(),
                            json!({ "data": data }),
                        ));
                    }
                    Err(_) => break,
                }
            }
        });

        let wait_session = session.clone();
        let wait_outbound = outbound;
        std::thread::spawn(move || {
            let code = child
                .wait()
                .ok()
                .map(|status| status.exit_code())
                .unwrap_or(0) as i32;
            wait_session.exited.store(true, Ordering::SeqCst);
            wait_session.busy.store(false, Ordering::SeqCst);
            let _ = wait_outbound.send(terminal_event(
                "terminal_exit",
                wait_session.id.as_str(),
                json!({ "code": code }),
            ));
        });

        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id, session.clone());
        Ok(session)
    }

    pub(crate) async fn get(&self, session_id: &str) -> Option<Arc<LocalPtySession>> {
        self.sessions.lock().await.get(session_id).cloned()
    }

    pub(crate) async fn close(&self, session_id: &str) {
        let session = self.sessions.lock().await.remove(session_id);
        if let Some(session) = session {
            session.close();
        }
    }
}
