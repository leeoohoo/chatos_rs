// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct RemoteConnectionQuery {
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateRemoteConnectionRequest {
    pub(super) name: Option<String>,
    pub(super) host: Option<String>,
    pub(super) port: Option<i64>,
    pub(super) username: Option<String>,
    pub(super) auth_type: Option<String>,
    pub(super) password: Option<String>,
    pub(super) private_key_path: Option<String>,
    pub(super) certificate_path: Option<String>,
    pub(super) default_remote_path: Option<String>,
    pub(super) host_key_policy: Option<String>,
    pub(super) local_connector_device_id: Option<String>,
    pub(super) local_connector_workspace_id: Option<String>,
    pub(super) jump_enabled: Option<bool>,
    pub(super) jump_connection_id: Option<String>,
    pub(super) jump_host: Option<String>,
    pub(super) jump_port: Option<i64>,
    pub(super) jump_username: Option<String>,
    pub(super) jump_private_key_path: Option<String>,
    pub(super) jump_certificate_path: Option<String>,
    pub(super) jump_password: Option<String>,
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateRemoteConnectionRequest {
    pub(super) name: Option<String>,
    pub(super) host: Option<String>,
    pub(super) port: Option<i64>,
    pub(super) username: Option<String>,
    pub(super) auth_type: Option<String>,
    pub(super) password: Option<String>,
    pub(super) private_key_path: Option<String>,
    pub(super) certificate_path: Option<String>,
    pub(super) default_remote_path: Option<String>,
    pub(super) host_key_policy: Option<String>,
    pub(super) local_connector_device_id: Option<String>,
    pub(super) local_connector_workspace_id: Option<String>,
    pub(super) jump_enabled: Option<bool>,
    pub(super) jump_connection_id: Option<String>,
    pub(super) jump_host: Option<String>,
    pub(super) jump_port: Option<i64>,
    pub(super) jump_username: Option<String>,
    pub(super) jump_private_key_path: Option<String>,
    pub(super) jump_certificate_path: Option<String>,
    pub(super) jump_password: Option<String>,
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
    #[serde(rename = "verification")]
    Verification { code: String },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub(super) enum WsOutput {
    #[serde(rename = "error")]
    Error {
        error: String,
        code: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        challenge_prompt: Option<String>,
    },
    #[serde(rename = "pong")]
    Pong { timestamp: String },
}
