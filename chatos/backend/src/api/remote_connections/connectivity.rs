// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use portable_pty::CommandBuilder;
use serde_json::Value;
use ssh2::Session;
use std::sync::mpsc;
use std::time::Duration as StdDuration;
use tokio::time::Duration;

use chatos_remote_runtime::establish_ssh_session;

use crate::api::local_connectors::run_remote_command_via_connector;
use crate::models::remote_connection::RemoteConnection;

use super::authenticate_target_session;
use super::build_ssh_args;
use super::configure_stream_timeout;
use super::connect_tcp_stream;
use super::create_jump_tunnel_stream_with_verification_channel;

const LOCAL_CONNECTOR_REQUIRED_FOR_KEY_AUTH: &str =
    "该远端连接依赖本地私钥/证书执行能力，云端后端不会再代替本机使用这些凭据";

pub(super) struct ConnectedSshSession {
    pub(super) session: Session,
}

pub(super) fn should_use_native_ssh(_connection: &RemoteConnection) -> bool {
    true
}

pub(super) fn connect_ssh2_session_with_verification(
    connection: &RemoteConnection,
    timeout_duration: Duration,
    verification_code: Option<&str>,
) -> Result<ConnectedSshSession, String> {
    ensure_cloud_safe_remote_connection(connection)?;
    connect_ssh2_session_with_interactive_verification(
        connection,
        timeout_duration,
        verification_code,
        None,
        None,
    )
}

pub(super) fn connect_ssh2_session_with_interactive_verification(
    connection: &RemoteConnection,
    timeout_duration: Duration,
    verification_code: Option<&str>,
    verification_code_rx: Option<mpsc::Receiver<String>>,
    challenge_tx: Option<mpsc::Sender<String>>,
) -> Result<ConnectedSshSession, String> {
    ensure_cloud_safe_remote_connection(connection)?;
    let timeout = StdDuration::from_millis(timeout_duration.as_millis().max(1) as u64);
    let timeout_ms = timeout_duration.as_millis().clamp(1000, u32::MAX as u128) as u32;
    let mut verification_code_rx = verification_code_rx;
    let mut challenge_tx = challenge_tx;
    let jump_enabled = connection.jump_enabled;
    let stream = if connection.jump_enabled {
        create_jump_tunnel_stream_with_verification_channel(
            connection,
            timeout,
            timeout_ms,
            verification_code,
            verification_code_rx.take(),
            challenge_tx.take(),
        )?
    } else {
        let stream =
            connect_tcp_stream(connection.host.as_str(), connection.port, timeout, "远端")?;
        configure_stream_timeout(&stream, timeout, "远端")?;
        stream
    };

    let session = establish_ssh_session(
        stream,
        timeout,
        connection.host.as_str(),
        connection.port,
        connection.host_key_policy.as_str(),
        |session| {
            let (target_verification_code_rx, target_challenge_tx) = if jump_enabled {
                (None, None)
            } else {
                (verification_code_rx, challenge_tx)
            };
            authenticate_target_session(
                session,
                connection,
                verification_code,
                target_verification_code_rx,
                target_challenge_tx,
            )
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(ConnectedSshSession { session })
}

fn ensure_cloud_safe_remote_connection(connection: &RemoteConnection) -> Result<(), String> {
    if connection.requires_local_credential_execution() {
        return Err(LOCAL_CONNECTOR_REQUIRED_FOR_KEY_AUTH.to_string());
    }
    Ok(())
}

pub(super) fn spawn_remote_shell(
    connection: &RemoteConnection,
    slave: Box<dyn portable_pty::SlavePty + Send>,
) -> Result<Box<dyn portable_pty::Child + Send + Sync>, String> {
    let mut cmd = CommandBuilder::new("ssh");
    let args = build_ssh_args(connection, true, connection.default_remote_path.as_deref());
    for arg in args {
        cmd.arg(arg);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    slave
        .spawn_command(cmd)
        .map_err(|e| format!("ssh spawn failed: {e}"))
}

pub(crate) async fn run_ssh_command(
    connection: &RemoteConnection,
    remote_command: &str,
    timeout_duration: Duration,
) -> Result<String, String> {
    run_ssh_command_with_verification(connection, remote_command, timeout_duration, None).await
}

pub(crate) async fn run_ssh_command_with_verification(
    connection: &RemoteConnection,
    remote_command: &str,
    timeout_duration: Duration,
    verification_code: Option<&str>,
) -> Result<String, String> {
    run_remote_command_via_connector(
        connection,
        remote_command,
        timeout_duration,
        verification_code,
    )
    .await
}

pub(crate) async fn run_remote_connectivity_test(
    connection: &RemoteConnection,
    verification_code: Option<&str>,
) -> Result<Value, String> {
    let script = "printf '__CHATOS_OK__\\n'; uname -n 2>/dev/null || hostname";
    let output = run_ssh_command_with_verification(
        connection,
        script,
        Duration::from_secs(12),
        verification_code,
    )
    .await?;
    if !output.contains("__CHATOS_OK__") {
        return Err("远端未返回预期握手标识".to_string());
    }

    let host_line = output
        .lines()
        .filter(|line| !line.contains("__CHATOS_OK__"))
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| connection.host.clone());

    Ok(serde_json::json!({
        "success": true,
        "remote_host": host_line,
        "connected_at": crate::core::time::now_rfc3339(),
    }))
}
