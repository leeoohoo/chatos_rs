use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use once_cell::sync::OnceCell;

use crate::models::terminal::{Terminal, TERMINAL_KIND_PROJECT_RUN};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::repositories::terminals;
use crate::services::realtime::{
    publish_project_run_instance_changed, publish_project_run_state_changed,
    publish_terminal_list_invalidated, publish_terminal_state_changed,
};

use super::{TerminalEvent, TerminalSession};

pub struct TerminalsManager {
    sessions: DashMap<String, Arc<TerminalSession>>,
}

impl TerminalsManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<TerminalSession>> {
        self.sessions.get(id).map(|s| s.clone())
    }

    pub fn get_busy(&self, id: &str) -> Option<bool> {
        self.sessions.get(id).map(|s| s.is_busy())
    }

    fn should_publish_terminal_list(terminal: &Terminal) -> bool {
        terminal.kind != TERMINAL_KIND_PROJECT_RUN
    }

    fn publish_list_invalidated_if_needed(
        terminal: &Terminal,
        reason: &str,
        terminal_payload: Option<&Terminal>,
    ) {
        if !Self::should_publish_terminal_list(terminal) {
            return;
        }
        if let Some(user_id) = terminal.user_id.as_deref() {
            publish_terminal_list_invalidated(
                user_id,
                Some(terminal.id.as_str()),
                terminal.project_id.as_deref(),
                reason,
                terminal_payload,
            );
        }
    }

    fn spawn_session(&self, terminal: &Terminal) -> Result<Arc<TerminalSession>, String> {
        let (session, mut child) = TerminalSession::new(terminal)?;
        let process_id = child.process_id().map(|value| value as i64);
        let id = terminal.id.clone();
        let sender = session.sender.clone();
        let handle = tokio::runtime::Handle::current();
        if process_id.is_some() {
            let id_for_pid = id.clone();
            let process_id_for_pid = process_id;
            handle.spawn(async move {
                let _ = terminals::update_terminal_status(
                    id_for_pid.as_str(),
                    Some("running".to_string()),
                    None,
                    process_id_for_pid,
                )
                .await;
            });
        }
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code()).unwrap_or(0) as i32;
            let _ = sender.send(TerminalEvent::Exit(code));
            let id_clone = id.clone();
            let handle = handle.clone();
            handle.spawn(async move {
                let manager = get_terminal_manager();
                manager.sessions.remove(&id_clone);
                let Some(existing_terminal) = terminals::get_terminal_by_id(&id_clone)
                    .await
                    .ok()
                    .flatten()
                else {
                    return;
                };

                let _ = terminals::update_terminal_status(
                    &id_clone,
                    Some("exited".to_string()),
                    None,
                    Some(0),
                )
                .await;
                if let Some(user_id) = existing_terminal.user_id.as_deref() {
                    let mut exited_terminal = existing_terminal.clone();
                    exited_terminal.status = "exited".to_string();
                    publish_terminal_state_changed(
                        user_id,
                        &exited_terminal,
                        false,
                        "process_exited",
                        Some(code),
                    );
                    Self::publish_list_invalidated_if_needed(
                        &existing_terminal,
                        "process_exited",
                        Some(&exited_terminal),
                    );
                    if let Some(project_id) = existing_terminal.project_id.as_deref() {
                        publish_project_run_instance_changed(
                            user_id,
                            project_id,
                            &exited_terminal,
                            false,
                            false,
                            "exited",
                            "process_exited",
                            Some(code),
                        );
                        publish_project_run_state_changed(
                            user_id,
                            project_id,
                            Some(&exited_terminal),
                            false,
                            false,
                            "exited",
                            "process_exited",
                            Some(code),
                        );
                    }
                }
            });
        });
        self.sessions.insert(terminal.id.clone(), session.clone());
        Ok(session)
    }

    pub async fn create(
        &self,
        name: String,
        cwd: String,
        kind: String,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Result<Terminal, String> {
        let terminal = Terminal::new(name, cwd, kind, user_id, project_id);
        terminals::create_terminal(&terminal).await?;
        let _ = self.spawn_session(&terminal)?;
        if let Some(user_id) = terminal.user_id.as_deref() {
            Self::publish_list_invalidated_if_needed(&terminal, "created", Some(&terminal));
            publish_terminal_state_changed(user_id, &terminal, false, "created", None);
            if let Some(project_id) = terminal.project_id.as_deref() {
                publish_project_run_instance_changed(
                    user_id, project_id, &terminal, false, true, "running", "created", None,
                );
                publish_project_run_state_changed(
                    user_id,
                    project_id,
                    Some(&terminal),
                    false,
                    true,
                    "running",
                    "created",
                    None,
                );
            }
        }
        Ok(terminal)
    }

    pub async fn ensure_running(
        &self,
        terminal: &Terminal,
    ) -> Result<Arc<TerminalSession>, String> {
        if let Some(session) = self.get(&terminal.id) {
            return Ok(session);
        }
        let session = self.spawn_session(terminal)?;
        let _ = terminals::update_terminal_status(
            &terminal.id,
            Some("running".to_string()),
            None,
            None,
        )
        .await;
        if let Some(user_id) = terminal.user_id.as_deref() {
            Self::publish_list_invalidated_if_needed(terminal, "ensured_running", Some(terminal));
            publish_terminal_state_changed(
                user_id,
                terminal,
                session.is_busy(),
                "ensured_running",
                None,
            );
            if let Some(project_id) = terminal.project_id.as_deref() {
                publish_project_run_instance_changed(
                    user_id,
                    project_id,
                    terminal,
                    session.is_busy(),
                    true,
                    "running",
                    "ensured_running",
                    None,
                );
                publish_project_run_state_changed(
                    user_id,
                    project_id,
                    Some(terminal),
                    session.is_busy(),
                    true,
                    "running",
                    "ensured_running",
                    None,
                );
            }
        }
        Ok(session)
    }

    pub async fn close(&self, id: &str) -> Result<(), String> {
        let terminal = terminals::get_terminal_by_id(id).await?;
        self.close_internal(id, terminal.as_ref(), true).await
    }

    pub async fn close_silently(&self, id: &str) -> Result<(), String> {
        let terminal = terminals::get_terminal_by_id(id).await?;
        self.close_internal(id, terminal.as_ref(), false).await
    }

    async fn close_internal(
        &self,
        id: &str,
        terminal: Option<&Terminal>,
        publish_events: bool,
    ) -> Result<(), String> {
        let session = self.sessions.get(id).map(|entry| entry.clone());
        if let Some(session) = session.as_ref() {
            let _ = session.terminate();
        }
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            if self.sessions.get(id).is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if self.sessions.get(id).is_some() {
            if let Some(session) = session.as_ref() {
                let _ = session.force_terminate();
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        self.sessions.remove(id);
        let _ =
            terminals::update_terminal_status(id, Some("exited".to_string()), None, Some(0)).await;
        if publish_events {
            if let Some(terminal) = terminal {
                if let Some(user_id) = terminal.user_id.as_deref() {
                    let mut exited_terminal = terminal.clone();
                    exited_terminal.status = "exited".to_string();
                    publish_terminal_state_changed(
                        user_id,
                        &exited_terminal,
                        false,
                        "closed",
                        None,
                    );
                    Self::publish_list_invalidated_if_needed(
                        terminal,
                        "closed",
                        Some(&exited_terminal),
                    );
                    if let Some(project_id) = terminal.project_id.as_deref() {
                        publish_project_run_instance_changed(
                            user_id,
                            project_id,
                            &exited_terminal,
                            false,
                            false,
                            "exited",
                            "closed",
                            None,
                        );
                        publish_project_run_state_changed(
                            user_id,
                            project_id,
                            Some(&exited_terminal),
                            false,
                            false,
                            "exited",
                            "closed",
                            None,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn close_project_run_terminals(
        &self,
        user_id: Option<&str>,
        project_id: &str,
    ) -> Result<usize, String> {
        let normalized_project_id = project_id.trim();
        if normalized_project_id.is_empty() {
            return Ok(0);
        }
        let terminals = crate::models::terminal::TerminalService::list_project_runs_by_project_id(
            user_id.map(|value| value.to_string()),
            normalized_project_id,
        )
        .await?;
        let mut closed = 0usize;
        for terminal in terminals {
            let _ = self
                .close_internal(terminal.id.as_str(), Some(&terminal), true)
                .await;
            let _ = TerminalLogService::create(TerminalLog::new(
                terminal.id.clone(),
                "signal".to_string(),
                "terminate:project_run_cleanup".to_string(),
            ))
            .await;
            let _ = crate::models::terminal::TerminalService::delete(terminal.id.as_str()).await;
            closed += 1;
        }
        Ok(closed)
    }

    pub async fn shutdown_all_project_run_terminals(&self) -> Result<usize, String> {
        let terminals =
            crate::models::terminal::TerminalService::list_by_kind(None, TERMINAL_KIND_PROJECT_RUN)
                .await?;
        let mut closed = 0usize;
        for terminal in terminals {
            let _ = self
                .close_internal(terminal.id.as_str(), Some(&terminal), false)
                .await;
            let _ = TerminalLogService::create(TerminalLog::new(
                terminal.id.clone(),
                "signal".to_string(),
                "terminate:server_shutdown".to_string(),
            ))
            .await;
            let _ = crate::models::terminal::TerminalService::delete(terminal.id.as_str()).await;
            closed += 1;
        }
        Ok(closed)
    }

    pub async fn cleanup_stale_project_run_terminals(&self) -> Result<usize, String> {
        let terminals =
            crate::models::terminal::TerminalService::list_by_kind(None, TERMINAL_KIND_PROJECT_RUN)
                .await?;
        let mut cleaned = 0usize;
        for terminal in terminals {
            let pid = terminal.process_id.unwrap_or(0);
            #[cfg(unix)]
            if pid > 0 {
                unsafe {
                    let _ = libc::kill(pid as i32, libc::SIGTERM);
                }
            }
            let _ = TerminalLogService::delete_by_terminal(terminal.id.as_str()).await;
            let _ = crate::models::terminal::TerminalService::delete(terminal.id.as_str()).await;
            cleaned += 1;
        }
        Ok(cleaned)
    }
}

static TERMINAL_MANAGER: OnceCell<Arc<TerminalsManager>> = OnceCell::new();

pub fn get_terminal_manager() -> Arc<TerminalsManager> {
    TERMINAL_MANAGER
        .get_or_init(|| Arc::new(TerminalsManager::new()))
        .clone()
}
