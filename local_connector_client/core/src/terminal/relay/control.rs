// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::approval::{
    approval_project_key_from_request, ApprovalDecision, CommandApprovalRequest,
    CommandApprovalService,
};
use crate::history::{command_history_entry_for_interactive_submission, CommandHistoryRecorder};
use crate::mcp::tools::request_project_root;
use crate::relay::{terminal_event, RelayRequest};
use crate::terminal::session::{LocalPtySession, LocalTerminalManager, PreparedTerminalInput};
use crate::workspace::paths::{relative_to_workspace, workspace_for_request};
use crate::LocalState;

use super::types::{
    TerminalSessionCloseRequest, TerminalSessionCommandRequest, TerminalSessionInputRequest,
    TerminalSessionResizeRequest, TerminalSessionSnapshotRequest,
};

pub(crate) async fn handle_terminal_input(
    value: Value,
    state: &LocalState,
    terminal_manager: &LocalTerminalManager,
    history_recorder: &CommandHistoryRecorder,
    outbound_tx: mpsc::UnboundedSender<Value>,
) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(input) = serde_json::from_value::<TerminalSessionInputRequest>(request.body.clone())
    else {
        return;
    };
    let Some(session) = terminal_manager
        .get(input.terminal_session_id.as_str())
        .await
    else {
        let _ = outbound_tx.send(terminal_event(
            "terminal_error",
            input.terminal_session_id.as_str(),
            json!({ "error": "terminal session not found" }),
        ));
        return;
    };
    if let Some(command) = input.command.as_deref() {
        session.set_submitted_command(command);
    }
    let prepared = match session.prepare_input(input.data.as_str()) {
        Ok(prepared) => prepared,
        Err(err) => {
            let _ = outbound_tx.send(terminal_event(
                "terminal_error",
                input.terminal_session_id.as_str(),
                json!({ "error": err.to_string() }),
            ));
            return;
        }
    };
    let (submissions, rejected) = match approve_prepared_terminal_input(
        &session,
        state,
        &request,
        history_recorder,
        prepared,
    )
    .await
    {
        Ok((submissions, rejected)) => (submissions, rejected),
        Err(err) => {
            let _ = outbound_tx.send(terminal_event(
                "terminal_error",
                input.terminal_session_id.as_str(),
                json!({ "error": err.to_string() }),
            ));
            return;
        }
    };
    for submission in submissions {
        history_recorder
            .append(command_history_entry_for_interactive_submission(
                state,
                &request,
                input.terminal_session_id.as_str(),
                submission,
            ))
            .await;
    }
    if rejected {
        let _ = outbound_tx.send(terminal_event(
            "terminal_state",
            input.terminal_session_id.as_str(),
            json!({ "busy": false }),
        ));
        return;
    }
    let _ = outbound_tx.send(terminal_event(
        "terminal_state",
        input.terminal_session_id.as_str(),
        json!({ "busy": session.busy() }),
    ));
}

pub(crate) async fn handle_terminal_command(value: Value, terminal_manager: &LocalTerminalManager) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(input) = serde_json::from_value::<TerminalSessionCommandRequest>(request.body) else {
        return;
    };
    let Some(session) = terminal_manager
        .get(input.terminal_session_id.as_str())
        .await
    else {
        return;
    };
    session.set_submitted_command(input.command.as_str());
}

async fn approve_prepared_terminal_input(
    session: &std::sync::Arc<LocalPtySession>,
    state: &LocalState,
    request: &RelayRequest,
    history_recorder: &CommandHistoryRecorder,
    mut prepared: PreparedTerminalInput,
) -> anyhow::Result<(
    Vec<crate::terminal::session::InteractiveCommandSubmission>,
    bool,
)> {
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let project_root = request_project_root(workspace, request)?;
    let project_root_label = relative_to_workspace(workspace, project_root.as_path());
    let mut denial_messages = Vec::new();
    for submission in prepared
        .submissions
        .iter_mut()
        .filter(|submission| submission.blocked_reason.is_none())
    {
        let cwd_label = relative_to_workspace(workspace, submission.cwd.as_path());
        let project_key = approval_project_key_from_request(
            state,
            request,
            workspace,
            project_root_label.clone(),
        );
        let approval = CommandApprovalService::new(
            history_recorder.state_path.clone(),
            history_recorder.state.clone(),
        )
        .approve(CommandApprovalRequest {
            request_id: request.request_id.clone(),
            project_key,
            command: submission.command.clone(),
            args: Vec::new(),
            cwd: cwd_label,
            source: "chatos_terminal_session".to_string(),
            requested_permissions: None,
            session_id: Some(session.id().to_string()),
        })
        .await?;
        if let ApprovalDecision::Denied { reason, .. } = approval {
            let message = format!("Blocked by command approval: {reason}");
            submission.blocked_reason = Some(message.clone());
            denial_messages.push(message);
        }
    }

    if !denial_messages.is_empty() {
        for submission in prepared
            .submissions
            .iter_mut()
            .filter(|submission| submission.blocked_reason.is_none())
        {
            submission.blocked_reason = Some(
                "Not executed because another command in the same input batch was denied."
                    .to_string(),
            );
        }
        let submissions = session.reject_prepared_input(prepared, denial_messages)?;
        return Ok((submissions, true));
    }

    Ok((session.commit_prepared_input(prepared)?, false))
}

pub(crate) async fn handle_terminal_resize(
    value: Value,
    terminal_manager: &LocalTerminalManager,
    outbound_tx: mpsc::UnboundedSender<Value>,
) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(resize) = serde_json::from_value::<TerminalSessionResizeRequest>(request.body) else {
        return;
    };
    let Some(session) = terminal_manager
        .get(resize.terminal_session_id.as_str())
        .await
    else {
        return;
    };
    if let Err(err) = session.resize(resize.cols, resize.rows) {
        let _ = outbound_tx.send(terminal_event(
            "terminal_error",
            resize.terminal_session_id.as_str(),
            json!({ "error": err.to_string() }),
        ));
    }
}

pub(crate) async fn handle_terminal_snapshot_request(
    value: Value,
    terminal_manager: &LocalTerminalManager,
    outbound_tx: mpsc::UnboundedSender<Value>,
) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(snapshot) = serde_json::from_value::<TerminalSessionSnapshotRequest>(request.body)
    else {
        return;
    };
    let Some(session) = terminal_manager
        .get(snapshot.terminal_session_id.as_str())
        .await
    else {
        return;
    };
    let data = session.snapshot(snapshot.lines.unwrap_or(500));
    let _ = outbound_tx.send(terminal_event(
        "terminal_snapshot",
        snapshot.terminal_session_id.as_str(),
        json!({ "data": data }),
    ));
}

pub(crate) async fn handle_terminal_close(value: Value, terminal_manager: &LocalTerminalManager) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(close) = serde_json::from_value::<TerminalSessionCloseRequest>(request.body) else {
        return;
    };
    terminal_manager
        .close(close.terminal_session_id.as_str())
        .await;
}
