// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::Serialize;
use tracing::warn;

use super::support::normalize_optional_text;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

#[derive(Debug, Serialize)]
struct TaskRunnerRemoteServerConfigHeader {
    name: String,
    host: String,
    port: i64,
    username: String,
    auth_type: String,
    password: Option<String>,
    private_key_path: Option<String>,
    certificate_path: Option<String>,
    default_remote_path: Option<String>,
    host_key_policy: String,
    enabled: bool,
}

pub(super) async fn build_task_runner_remote_server_config_header(
    effective_user_id: Option<&str>,
    remote_connection_id: Option<&str>,
) -> Option<String> {
    let remote_connection_id = normalize_optional_text(remote_connection_id)?;
    let connection = match RemoteConnectionService::get_by_id(remote_connection_id.as_str()).await {
        Ok(Some(connection)) => connection,
        Ok(None) => {
            warn!(
                "task runner remote passthrough skipped: remote connection missing: {}",
                remote_connection_id
            );
            return None;
        }
        Err(err) => {
            warn!(
                "task runner remote passthrough skipped: load remote connection failed: id={} detail={}",
                remote_connection_id, err
            );
            return None;
        }
    };
    if let Some(user_id) = effective_user_id {
        if connection.user_id.as_deref() != Some(user_id) {
            warn!(
                "task runner remote passthrough skipped: remote connection forbidden: id={}",
                remote_connection_id
            );
            return None;
        }
    }
    let payload = task_runner_remote_server_config_from_connection(connection);
    match serde_json::to_vec(&payload) {
        Ok(bytes) => Some(URL_SAFE_NO_PAD.encode(bytes)),
        Err(err) => {
            warn!(
                "task runner remote passthrough skipped: encode remote server config failed: {}",
                err
            );
            None
        }
    }
}

fn task_runner_remote_server_config_from_connection(
    connection: RemoteConnection,
) -> TaskRunnerRemoteServerConfigHeader {
    TaskRunnerRemoteServerConfigHeader {
        name: connection.name,
        host: connection.host,
        port: connection.port,
        username: connection.username,
        auth_type: connection.auth_type,
        password: connection.password,
        private_key_path: connection.private_key_path,
        certificate_path: connection.certificate_path,
        default_remote_path: connection.default_remote_path,
        host_key_policy: connection.host_key_policy,
        enabled: true,
    }
}
