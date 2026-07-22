// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl TaskRunnerTerminalControllerStore {
    pub(super) async fn process_write_value(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        data: String,
        submit: bool,
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        refresh_session_status(&session).await?;
        let mut child = session.child.lock().await;
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "terminal stdin is unavailable".to_string())?;
        stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|err| err.to_string())?;
        if submit {
            stdin
                .write_all(b"\n")
                .await
                .map_err(|err| err.to_string())?;
        }
        stdin.flush().await.map_err(|err| err.to_string())?;
        drop(child);
        let mut content = data.clone();
        if submit {
            content.push('\n');
        }
        append_log(session.clone(), "input", content).await;
        Ok(json!({
            "ok": true,
            "terminal_id": terminal_id,
            "bytes_written": data.len() + usize::from(submit),
            "submit": submit,
        }))
    }

    pub(super) async fn process_kill_value(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        {
            let mut child = session.child.lock().await;
            terminate_task_terminal_process_tree(&mut child).await?;
        }
        mark_session_exited(&session, None).await;
        append_log(session.clone(), "system", "[terminal killed]\n".to_string()).await;
        Ok(json!({
            "ok": true,
            "terminal_id": terminal_id,
            "killed": true,
        }))
    }
}
