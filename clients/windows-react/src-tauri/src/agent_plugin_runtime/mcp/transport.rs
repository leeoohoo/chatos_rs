// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::McpSession;
use serde_json::{json, Value};
use std::{collections::HashSet, time::Duration};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
#[cfg(not(windows))]
use tokio::process::{Child, ChildStdin, ChildStdout};

use crate::machine_resource_governor::MachineResourcePermit;
#[cfg(not(windows))]
use crate::terminal_runtime::process::ProcessTermination;
#[cfg(windows)]
use crate::windows_appcontainer::SandboxedChild;

const MCP_TIMEOUT: Duration = Duration::from_secs(30);
const PROCESS_REAP_TIMEOUT: Duration = Duration::from_secs(2);
const MAXIMUM_MCP_RESPONSE_BYTES: usize = 4 * 1_024 * 1_024;
const MAXIMUM_MCP_TOOL_PAGES: usize = 100;
const MAXIMUM_MCP_TOOLS: usize = 512;

#[cfg(windows)]
pub(super) type McpStdin = tokio::fs::File;
#[cfg(not(windows))]
type McpStdin = ChildStdin;
#[cfg(windows)]
pub(super) type McpStdout = tokio::fs::File;
#[cfg(not(windows))]
type McpStdout = ChildStdout;

pub(in crate::agent_plugin_runtime) struct McpConnection {
    #[cfg(windows)]
    pub(super) child: Option<SandboxedChild>,
    #[cfg(not(windows))]
    pub(super) child: Child,
    #[cfg(not(windows))]
    pub(super) _termination: ProcessTermination,
    pub(super) stdin: McpStdin,
    pub(super) stdout: BufReader<McpStdout>,
    #[cfg(windows)]
    pub(super) stderr_task: tokio::task::JoinHandle<()>,
    pub(super) resource_permit: Option<MachineResourcePermit>,
    pub(super) next_request_id: u64,
    pub(super) unusable: bool,
}

impl McpSession {
    pub(super) async fn list_tools(&self) -> Result<Vec<Value>, McpError> {
        let mut tools = Vec::new();
        let mut cursor = None;
        let mut cursors = HashSet::new();
        for _ in 0..MAXIMUM_MCP_TOOL_PAGES {
            let parameters = cursor
                .as_ref()
                .map(|cursor| json!({"cursor": cursor}))
                .unwrap_or_else(|| json!({}));
            let result = self.request("tools/list", parameters).await?;
            let page = result
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| McpError::known("Plugin tools/list response is invalid."))?;
            if tools.len().saturating_add(page.len()) > MAXIMUM_MCP_TOOLS {
                return Err(McpError::known(
                    "Plugin tool catalog exceeds the safety limit.",
                ));
            }
            tools.extend(page.iter().cloned());
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let Some(next) = cursor.as_ref() else {
                return Ok(tools);
            };
            if !cursors.insert(next.clone()) {
                return Err(McpError::known("Plugin returned a repeated tools cursor."));
            }
        }
        Err(McpError::known("Plugin tool catalog has too many pages."))
    }

    pub(in crate::agent_plugin_runtime) async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<Value, McpError> {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
            .await
    }

    pub(super) async fn request(&self, method: &str, parameters: Value) -> Result<Value, McpError> {
        let future = async {
            let Some(connection) = self.connection.as_ref() else {
                return Err(McpError::known("Plugin does not provide an MCP runtime."));
            };
            let mut connection = connection.lock().await;
            if connection.unusable {
                return Err(McpError::unknown("Plugin MCP session is unavailable."));
            }
            let request_id = connection.next_request_id;
            connection.next_request_id = connection.next_request_id.saturating_add(1);
            write_message(
                &mut connection.stdin,
                &json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "method": method,
                    "params": parameters,
                }),
            )
            .await?;
            loop {
                let response = read_message(&mut connection.stdout).await?;
                if response.get("id").and_then(Value::as_u64) != Some(request_id) {
                    continue;
                }
                if let Some(error) = response.get("error") {
                    return Err(McpError::known(format!(
                        "Plugin MCP {method} failed: {error}"
                    )));
                }
                return response
                    .get("result")
                    .cloned()
                    .ok_or_else(|| McpError::known("Plugin MCP response has no result."));
            }
        };
        match tokio::time::timeout(MCP_TIMEOUT, future).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) if error.outcome_known => Err(error),
            Ok(Err(error)) => {
                self.invalidate().await;
                Err(error)
            }
            Err(_) => {
                self.invalidate().await;
                Err(McpError::unknown("Plugin MCP request timed out."))
            }
        }
    }

    pub(super) async fn notify(&self, method: &str, parameters: Value) -> Result<(), McpError> {
        let future = async {
            let Some(connection) = self.connection.as_ref() else {
                return Err(McpError::known("Plugin does not provide an MCP runtime."));
            };
            let mut connection = connection.lock().await;
            if connection.unusable {
                return Err(McpError::unknown("Plugin MCP session is unavailable."));
            }
            write_message(
                &mut connection.stdin,
                &json!({"jsonrpc": "2.0", "method": method, "params": parameters}),
            )
            .await
        };
        match tokio::time::timeout(MCP_TIMEOUT, future).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) if error.outcome_known => Err(error),
            Ok(Err(error)) => {
                self.invalidate().await;
                Err(error)
            }
            Err(_) => {
                self.invalidate().await;
                Err(McpError::unknown("Plugin MCP notification timed out."))
            }
        }
    }

    pub(in crate::agent_plugin_runtime) async fn invalidate(&self) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let mut connection = connection.lock().await;
        if connection.unusable {
            return;
        }
        connection.unusable = true;
        connection.resource_permit.take();
        #[cfg(windows)]
        {
            if let Some(child) = connection.child.take() {
                let _ = child.terminate();
                let _ = tokio::task::spawn_blocking(move || child.wait(PROCESS_REAP_TIMEOUT)).await;
            }
            connection.stderr_task.abort();
        }
        #[cfg(not(windows))]
        {
            let _ = connection._termination.terminate(true);
            let _ = connection.child.start_kill();
            let _ = tokio::time::timeout(PROCESS_REAP_TIMEOUT, connection.child.wait()).await;
        }
    }
}

#[derive(Debug)]
pub(in crate::agent_plugin_runtime) struct McpError {
    pub(in crate::agent_plugin_runtime) message: String,
    pub(in crate::agent_plugin_runtime) outcome_known: bool,
}

impl McpError {
    fn known(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            outcome_known: true,
        }
    }

    fn unknown(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            outcome_known: false,
        }
    }
}

async fn write_message(
    stdin: &mut (impl AsyncWrite + Unpin),
    value: &Value,
) -> Result<(), McpError> {
    let mut payload = serde_json::to_vec(value)
        .map_err(|_| McpError::known("Plugin MCP request could not be encoded."))?;
    payload.push(b'\n');
    stdin
        .write_all(&payload)
        .await
        .map_err(|_| McpError::unknown("Plugin MCP request could not be written."))?;
    stdin
        .flush()
        .await
        .map_err(|_| McpError::unknown("Plugin MCP request could not be flushed."))
}

async fn read_message(stdout: &mut (impl AsyncBufRead + Unpin)) -> Result<Value, McpError> {
    let mut payload = Vec::new();
    loop {
        let available = stdout
            .fill_buf()
            .await
            .map_err(|_| McpError::unknown("Plugin MCP response could not be read."))?;
        if available.is_empty() {
            return Err(McpError::unknown(
                "Plugin MCP process closed before responding.",
            ));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let content_length = newline.unwrap_or(available.len());
        if payload.len().saturating_add(content_length) > MAXIMUM_MCP_RESPONSE_BYTES {
            return Err(McpError::unknown("Plugin MCP response is too large."));
        }
        payload.extend_from_slice(&available[..content_length]);
        stdout.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    serde_json::from_slice(&payload)
        .map_err(|_| McpError::unknown("Plugin MCP response is not valid JSON."))
}
