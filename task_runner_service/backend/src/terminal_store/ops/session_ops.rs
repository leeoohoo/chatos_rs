// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl TaskRunnerTerminalControllerStore {
    pub async fn start_shell_session(
        &self,
        context: TerminalControllerContext,
        path: String,
    ) -> Result<Value, String> {
        let project_root = canonicalize_existing(context.root.as_path())?;
        let target_path = resolve_target_path(project_root.as_path(), path.as_str())?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

        let mut process = Command::new(shell.as_str());
        process
            .current_dir(&target_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        apply_bundled_tools_path(&mut process);
        let child = process.spawn().map_err(|err| err.to_string())?;

        let session = register_session(
            context.clone(),
            target_path.clone(),
            format!("task terminal shell: {shell}"),
            child,
        )
        .await?;
        append_log(
            session.clone(),
            "system",
            "[task terminal shell started]\n".to_string(),
        )
        .await;
        let meta = session.meta.lock().await.clone();
        Ok(json!({
            "project_root": project_root.to_string_lossy(),
            "terminal_id": meta.id,
            "process_id": meta.id,
            "path": target_path.to_string_lossy(),
            "command": meta.command,
            "background": true,
            "busy": true,
            "status": meta.status,
            "started_at": meta.started_at,
            "project_id": meta.project_id,
            "user_id": meta.user_id,
        }))
    }

    pub async fn kill_sessions_for_context(
        &self,
        context: TerminalControllerContext,
    ) -> Result<Value, String> {
        let sessions = sessions_for_context(&context).await?;
        let total = sessions.len();
        let mut killed = 0usize;
        let mut already_exited = 0usize;
        let mut errors = Vec::new();
        let mut terminal_ids = Vec::new();

        for session in sessions {
            if let Err(err) = refresh_session_status(&session).await {
                errors.push(err);
                continue;
            }
            let meta = session.meta.lock().await.clone();
            terminal_ids.push(meta.id.clone());
            if meta.status == "exited" {
                already_exited += 1;
                continue;
            }
            {
                let mut child = session.child.lock().await;
                if let Err(err) = child.kill().await {
                    errors.push(format!("kill {} failed: {}", meta.id, err));
                    continue;
                }
                let _ = child.wait().await;
            }
            mark_session_exited(&session, None).await;
            append_log(
                session.clone(),
                "system",
                "[task terminal cleanup killed process]\n".to_string(),
            )
            .await;
            killed += 1;
        }

        Ok(json!({
            "ok": errors.is_empty(),
            "total": total,
            "killed": killed,
            "already_exited": already_exited,
            "terminal_ids": terminal_ids,
            "errors": errors,
        }))
    }
}
