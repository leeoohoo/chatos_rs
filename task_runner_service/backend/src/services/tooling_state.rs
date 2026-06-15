use std::path::PathBuf;

use serde_json::{Value, json};

use chatos_builtin_tools::{NotepadStore, TerminalControllerContext, TerminalControllerStore};

use crate::config::AppConfig;
use crate::notepad_store::TaskRunnerNotepadStore;
use crate::terminal_store::TaskRunnerTerminalControllerStore;

use super::{ToolingStateService, normalized_optional};

impl ToolingStateService {
    pub(crate) fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn list_notepad_folders(&self, user_id: Option<&str>) -> Result<Value, String> {
        self.notepad_store(user_id)?.list_folders().await
    }

    pub async fn list_notepad_notes(
        &self,
        user_id: Option<&str>,
        folder: Option<String>,
        tags: Vec<String>,
        query: Option<String>,
        limit: Option<usize>,
        match_any: bool,
        recursive: bool,
    ) -> Result<Value, String> {
        self.notepad_store(user_id)?
            .list_notes(json!({
                "folder": folder,
                "recursive": recursive,
                "tags": tags,
                "match_any": match_any,
                "query": query,
                "limit": limit.unwrap_or(200).clamp(1, 500),
            }))
            .await
    }

    pub async fn read_notepad_note(
        &self,
        user_id: Option<&str>,
        note_id: &str,
    ) -> Result<Value, String> {
        self.notepad_store(user_id)?.read_note(note_id).await
    }

    pub async fn list_notepad_tags(&self, user_id: Option<&str>) -> Result<Value, String> {
        self.notepad_store(user_id)?.list_tags().await
    }

    pub async fn list_terminal_processes(
        &self,
        user_id: Option<String>,
        project_id: Option<String>,
        include_exited: bool,
        limit: usize,
    ) -> Result<Value, String> {
        TaskRunnerTerminalControllerStore
            .process_list(
                self.terminal_context(user_id, project_id),
                include_exited,
                limit.clamp(1, 100),
            )
            .await
    }

    pub async fn get_terminal_process_logs(
        &self,
        terminal_id: &str,
        user_id: Option<String>,
        project_id: Option<String>,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Value, String> {
        TaskRunnerTerminalControllerStore
            .process_poll(
                self.terminal_context(user_id, project_id),
                terminal_id.to_string(),
                offset,
                limit.unwrap_or(200).clamp(1, 200),
            )
            .await
    }

    pub async fn kill_terminal_process(
        &self,
        terminal_id: &str,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Result<Value, String> {
        TaskRunnerTerminalControllerStore
            .process_kill(
                self.terminal_context(user_id, project_id),
                terminal_id.to_string(),
            )
            .await
    }

    pub async fn write_terminal_process(
        &self,
        terminal_id: &str,
        user_id: Option<String>,
        project_id: Option<String>,
        data: String,
        submit: bool,
    ) -> Result<Value, String> {
        TaskRunnerTerminalControllerStore
            .process_write(
                self.terminal_context(user_id, project_id),
                terminal_id.to_string(),
                data,
                submit,
            )
            .await
    }

    fn notepad_store(&self, user_id: Option<&str>) -> Result<TaskRunnerNotepadStore, String> {
        let root = PathBuf::from(&self.config.default_workspace_dir)
            .join(".task_runner")
            .join("notepad");
        TaskRunnerNotepadStore::new(root, user_id.unwrap_or("task_runner"))
    }

    fn terminal_context(
        &self,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> TerminalControllerContext {
        TerminalControllerContext {
            root: PathBuf::from(&self.config.default_workspace_dir),
            user_id: normalized_optional(user_id),
            project_id: normalized_optional(project_id),
            idle_timeout_ms: 5_000,
            max_wait_ms: 60_000,
            max_output_chars: 20_000,
        }
    }
}
