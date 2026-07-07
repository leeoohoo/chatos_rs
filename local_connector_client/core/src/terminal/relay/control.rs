// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::history::{command_history_entry_for_interactive_submission, CommandHistoryRecorder};
use crate::relay::{terminal_event, RelayRequest};
use crate::terminal::session::LocalTerminalManager;
use crate::LocalState;

use super::types::{
    TerminalSessionCloseRequest, TerminalSessionInputRequest, TerminalSessionResizeRequest,
    TerminalSessionSnapshotRequest,
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
    let submissions = match session.write_input(input.data.as_str()) {
        Ok(submissions) => submissions,
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
    let _ = outbound_tx.send(terminal_event(
        "terminal_state",
        input.terminal_session_id.as_str(),
        json!({ "busy": session.busy() }),
    ));
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
