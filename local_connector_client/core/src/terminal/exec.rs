// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::history::{CommandExecutionContext, CommandHistoryRecorder};
use crate::relay::{relay_error_response, RelayRequest, RelayResponse};
use crate::LocalState;

mod runner;

use runner::run_terminal_exec;

pub(crate) async fn handle_terminal_exec_request(
    value: Value,
    state: &LocalState,
    history_recorder: &CommandHistoryRecorder,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("terminal_response", "", 400, err.to_string());
        }
    };
    match run_terminal_exec(
        &request,
        state,
        request.body.clone(),
        CommandExecutionContext::terminal_exec(&request),
        Some(history_recorder),
    )
    .await
    {
        Ok(body) => {
            let status = if body
                .get("timed_out")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                408
            } else {
                200
            };
            RelayResponse {
                message_type: "terminal_response".to_string(),
                request_id: request.request_id,
                status,
                headers: BTreeMap::new(),
                body,
            }
            .into_value()
        }
        Err(err) => RelayResponse {
            message_type: "terminal_response".to_string(),
            request_id: request.request_id,
            status: 400,
            headers: BTreeMap::new(),
            body: json!({ "error": err.to_string() }),
        }
        .into_value(),
    }
}
