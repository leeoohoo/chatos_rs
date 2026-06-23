use super::*;

impl TaskRunnerTerminalControllerStore {
    pub(super) async fn execute_command_value(
        &self,
        context: TerminalControllerContext,
        path: String,
        command: String,
        background: bool,
    ) -> Result<Value, String> {
        let project_root = canonicalize_existing(context.root.as_path())?;
        let target_path = resolve_target_path(project_root.as_path(), path.as_str())?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

        let mut process = Command::new(shell);
        process
            .arg("-lc")
            .arg(command.as_str())
            .current_dir(&target_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        apply_bundled_tools_path(&mut process);
        let child = process.spawn().map_err(|err| err.to_string())?;

        let session =
            register_session(context.clone(), target_path.clone(), command.clone(), child).await?;
        append_log(session.clone(), "command", command.clone()).await;
        let session_id = session.meta.lock().await.id.clone();

        if background {
            return Ok(json!({
                "project_root": project_root.to_string_lossy(),
                "terminal_id": session_id,
                "process_id": session_id,
                "path": target_path.to_string_lossy(),
                "common": command,
                "background": true,
                "busy": true,
                "output": "",
                "output_chars": 0,
                "truncated": false,
                "finished_by": "background",
                "idle_timeout_ms": context.idle_timeout_ms,
                "max_wait_ms": context.max_wait_ms,
                "max_output_chars": context.max_output_chars
            }));
        }

        let wait_result = wait_for_session(session.clone(), context.max_wait_ms).await?;
        let output = collect_output(&session, context.max_output_chars).await;
        Ok(json!({
            "project_root": project_root.to_string_lossy(),
            "terminal_id": session_id.clone(),
            "process_id": session_id,
            "path": target_path.to_string_lossy(),
            "common": command,
            "background": false,
            "busy": wait_result.busy,
            "output": output.text,
            "output_chars": output.char_count,
            "truncated": output.truncated,
            "finished_by": wait_result.finished_by,
            "exit_code": wait_result.exit_code,
            "idle_timeout_ms": context.idle_timeout_ms,
            "max_wait_ms": context.max_wait_ms,
            "max_output_chars": context.max_output_chars
        }))
    }
}
