use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::OnceCell;

use crate::models::terminal::Terminal;
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
        let terminal_for_events = terminal.clone();
        let sender = session.sender.clone();
        let handle = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code()).unwrap_or(0) as i32;
            let _ = sender.send(TerminalEvent::Exit(code));
            let id_clone = id.clone();
            let handle = handle.clone();
            handle.spawn(async move {
                let _ =
                    terminals::update_terminal_status(&id_clone, Some("exited".to_string()), None)
                        .await;
                if let Some(user_id) = terminal_for_events.user_id.as_deref() {
                    publish_terminal_state_changed(
                        user_id,
                        &terminal_for_events,
                        false,
                        "process_exited",
                    );
                    publish_terminal_list_invalidated(
                        user_id,
                        Some(terminal_for_events.id.as_str()),
                        terminal_for_events.project_id.as_deref(),
                        "process_exited",
                    );
                    if let Some(project_id) = terminal_for_events.project_id.as_deref() {
                        publish_project_run_state_changed(
                            user_id,
                            project_id,
                            Some(&terminal_for_events),
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
        if let Some(session) = self.sessions.remove(id).map(|(_, s)| s) {
            let _ = session.write_input("exit\n");
        }
        terminals::update_terminal_status(id, Some("exited".to_string()), None).await?;
        if let Some(terminal) = terminal {
            if let Some(user_id) = terminal.user_id.as_deref() {
                publish_terminal_state_changed(user_id, &terminal, false, "closed");
                publish_terminal_list_invalidated(
                    user_id,
                    Some(terminal.id.as_str()),
                    terminal.project_id.as_deref(),
                    "closed",
                );
                if let Some(project_id) = terminal.project_id.as_deref() {
                    publish_project_run_state_changed(
                        user_id,
                        project_id,
                        Some(&terminal),
                        false,
                        false,
                        "exited",
                        "closed",
                    );
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
