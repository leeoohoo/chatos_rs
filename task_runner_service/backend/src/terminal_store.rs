use tokio::process::Child;
use tokio::sync::{Mutex, RwLock};

mod ops;
mod output;
mod pathing;
mod runtime;

#[derive(Debug, Clone)]
struct TerminalLogEntry {
    offset: i64,
    kind: String,
    content: String,
    created_at: String,
}

#[derive(Debug, Clone)]
struct TerminalSessionMeta {
    id: String,
    cwd: String,
    project_id: Option<String>,
    user_id: Option<String>,
    command: String,
    started_at: String,
    last_active_at: String,
    finished_at: Option<String>,
    status: String,
    exit_code: Option<i32>,
}

struct TerminalSession {
    meta: Mutex<TerminalSessionMeta>,
    child: Mutex<Child>,
    logs: Mutex<Vec<TerminalLogEntry>>,
}

#[derive(Default)]
struct TerminalRuntimeState {
    sessions: RwLock<std::collections::HashMap<String, std::sync::Arc<TerminalSession>>>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskRunnerTerminalControllerStore;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use chatos_builtin_tools::{TerminalControllerContext, TerminalControllerStore};
    use serde_json::Value;

    use super::*;

    fn unique_id(prefix: &str) -> String {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        format!("{prefix}-{}-{unique}", std::process::id())
    }

    fn test_context(root: PathBuf, project_id: String) -> TerminalControllerContext {
        TerminalControllerContext {
            root,
            user_id: Some(unique_id("user")),
            project_id: Some(project_id),
            idle_timeout_ms: 1_000,
            max_wait_ms: 1_000,
            max_output_chars: 4_000,
        }
    }

    #[tokio::test]
    async fn kill_sessions_for_context_stops_running_task_sessions() {
        let root = std::env::temp_dir().join(unique_id("task-terminal-cleanup"));
        std::fs::create_dir_all(&root).expect("create temp root");
        let store = TaskRunnerTerminalControllerStore;
        let context = test_context(root, unique_id("project"));

        let started = store
            .execute_command(
                context.clone(),
                ".".to_string(),
                "sleep 60".to_string(),
                true,
            )
            .await
            .expect("start background command");
        assert_eq!(started.get("busy").and_then(Value::as_bool), Some(true));

        let cleanup = store
            .kill_sessions_for_context(context.clone())
            .await
            .expect("cleanup sessions");
        assert_eq!(cleanup.get("killed").and_then(Value::as_u64), Some(1));

        let listed = store
            .process_list(context, false, 10)
            .await
            .expect("list running sessions");
        assert_eq!(listed.get("process_count").and_then(Value::as_u64), Some(0));
    }
}
