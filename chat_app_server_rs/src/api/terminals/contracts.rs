use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct TerminalQuery {
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateTerminalRequest {
    pub(super) name: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalLogQuery {
    pub(super) limit: Option<i64>,
    pub(super) offset: Option<i64>,
    pub(super) before: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DispatchTerminalCommandRequest {
    pub(super) cwd: Option<String>,
    pub(super) command: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) create_if_missing: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(super) enum WsInput {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "command")]
    Command { command: String },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
    #[serde(rename = "snapshot")]
    Snapshot { lines: Option<usize> },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub(super) enum WsOutput {
    #[serde(rename = "output")]
    Output { data: String },
    #[serde(rename = "snapshot")]
    Snapshot { data: String },
    #[serde(rename = "exit")]
    Exit { code: i32 },
    #[serde(rename = "state")]
    State { busy: bool, snapshot_paging: bool },
    #[serde(rename = "error")]
    Error { error: String },
    #[serde(rename = "pong")]
    Pong { timestamp: String },
}
