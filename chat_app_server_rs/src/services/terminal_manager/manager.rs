use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::OnceCell;

use crate::models::terminal::Terminal;
use crate::repositories::terminal_logs;
use crate::repositories::terminals;
use crate::services::realtime::{
    publish_project_run_state_changed, publish_terminal_list_invalidated,
    publish_terminal_state_changed,
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

    fn spawn_session(&self, terminal: &Terminal) -> Result<Arc<TerminalSession>, String> {
        let (session, mut child) = TerminalSession::new(terminal)?;
        let id = terminal.id.clone();
        let sender = session.sender.clone();
        let handle = tokio::runtime::Handle::current();
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

                let _ = terminal_logs::delete_terminal_logs(&id_clone).await;
                let _ = terminals::delete_terminal(&id_clone).await;
                if let Some(user_id) = existing_terminal.user_id.as_deref() {
                    let mut exited_terminal = existing_terminal.clone();
                    exited_terminal.status = "exited".to_string();
                    publish_terminal_state_changed(
                        user_id,
                        &exited_terminal,
                        false,
                        "process_exited",
                    );
                    publish_terminal_list_invalidated(
                        user_id,
                        Some(existing_terminal.id.as_str()),
                        existing_terminal.project_id.as_deref(),
                        "deleted",
                        None,
                    );
                    if let Some(project_id) = existing_terminal.project_id.as_deref() {
                        publish_project_run_state_changed(
                            user_id,
                            project_id,
                            Some(&exited_terminal),
                            false,
                            false,
                            "exited",
                            "process_exited",
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
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Result<Terminal, String> {
        let terminal = Terminal::new(name, cwd, user_id, project_id);
        terminals::create_terminal(&terminal).await?;
        let _ = self.spawn_session(&terminal)?;
        if let Some(user_id) = terminal.user_id.as_deref() {
            publish_terminal_list_invalidated(
                user_id,
                Some(terminal.id.as_str()),
                terminal.project_id.as_deref(),
                "created",
                Some(&terminal),
            );
            publish_terminal_state_changed(user_id, &terminal, false, "created");
            if let Some(project_id) = terminal.project_id.as_deref() {
                publish_project_run_state_changed(
                    user_id,
                    project_id,
                    Some(&terminal),
                    false,
                    true,
                    "running",
                    "created",
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
        let _ = terminals::update_terminal_status(&terminal.id, Some("running".to_string()), None)
            .await;
        if let Some(user_id) = terminal.user_id.as_deref() {
            publish_terminal_list_invalidated(
                user_id,
                Some(terminal.id.as_str()),
                terminal.project_id.as_deref(),
                "ensured_running",
                Some(terminal),
            );
            publish_terminal_state_changed(user_id, terminal, session.is_busy(), "ensured_running");
            if let Some(project_id) = terminal.project_id.as_deref() {
                publish_project_run_state_changed(
                    user_id,
                    project_id,
                    Some(terminal),
                    session.is_busy(),
                    true,
                    "running",
                    "ensured_running",
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
        if let Some(session) = self.sessions.remove(id).map(|(_, s)| s) {
            let _ = session.write_input("exit\n");
        }
        let _ = terminal_logs::delete_terminal_logs(id).await;
        terminals::delete_terminal(id).await?;
        if publish_events {
            if let Some(terminal) = terminal {
                if let Some(user_id) = terminal.user_id.as_deref() {
                    let mut exited_terminal = terminal.clone();
                    exited_terminal.status = "exited".to_string();
                    publish_terminal_state_changed(user_id, &exited_terminal, false, "closed");
                    publish_terminal_list_invalidated(
                        user_id,
                        Some(terminal.id.as_str()),
                        terminal.project_id.as_deref(),
                        "closed",
                        None,
                    );
                    if let Some(project_id) = terminal.project_id.as_deref() {
                        publish_project_run_state_changed(
                            user_id,
                            project_id,
                            Some(&exited_terminal),
                            false,
                            false,
                            "exited",
                            "closed",
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

static TERMINAL_MANAGER: OnceCell<Arc<TerminalsManager>> = OnceCell::new();

pub fn get_terminal_manager() -> Arc<TerminalsManager> {
    TERMINAL_MANAGER
        .get_or_init(|| Arc::new(TerminalsManager::new()))
        .clone()
}
