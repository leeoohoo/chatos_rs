// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use portable_pty::CommandBuilder;
use serde_json::Value;
use ssh2::Session;
use std::io::Read;
use std::sync::mpsc;
use std::time::Duration as StdDuration;
use tokio::time::Duration;

use crate::models::remote_connection::RemoteConnection;
use crate::utils::process_output::{run_command_limited, BoundedCommandError};

use super::apply_host_key_policy;
use super::authenticate_target_session;
use super::build_ssh_args;
use super::build_ssh_process_command;
use super::configure_stream_timeout;
use super::connect_tcp_stream;
use super::create_jump_tunnel_stream_with_verification_channel;
use super::encode_second_factor_required_error;
use super::is_password_auth;
use super::map_command_spawn_error;

const REMOTE_SSH_STDOUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const REMOTE_SSH_STDERR_LIMIT_BYTES: usize = 1024 * 1024;

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

    let mut session = Session::new().map_err(|e| format!("创建 SSH 会话失败: {e}"))?;
    session.set_tcp_stream(stream);
    session.set_timeout(timeout_ms);
    session
        .handshake()
        .map_err(|e| format!("SSH 握手失败: {e}"))?;
    apply_host_key_policy(
        &session,
        connection.host.as_str(),
        connection.port,
        connection.host_key_policy.as_str(),
    )?;
    let (target_verification_code_rx, target_challenge_tx) = if jump_enabled {
        (None, None)
    } else {
        (verification_code_rx, challenge_tx)
    };
    authenticate_target_session(
        &session,
        connection,
        verification_code,
        target_verification_code_rx,
        target_challenge_tx,
    )?;

    if !session.authenticated() {
        return Err("SSH 认证失败".to_string());
    }

    Ok(ConnectedSshSession { session })
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
    if should_use_native_ssh(connection) {
        let connection = connection.clone();
        let command = remote_command.to_string();
        let timeout_duration_copy = timeout_duration;
        let verification_code_owned = verification_code
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned);
        return tokio::task::spawn_blocking(move || {
            let connected = connect_ssh2_session_with_verification(
                &connection,
                timeout_duration_copy,
                verification_code_owned.as_deref(),
            )?;
            let mut channel = connected
                .session
                .channel_session()
                .map_err(|e| format!("创建命令通道失败: {e}"))?;
            channel
                .exec(command.as_str())
                .map_err(|e| format!("执行远端命令失败: {e}"))?;

            let stdout =
                read_ssh_stream_limited(&mut channel, REMOTE_SSH_STDOUT_LIMIT_BYTES, "stdout")
                    .map_err(|e| format!("读取标准输出失败: {e}"))?;
            let mut stderr_stream = channel.stderr();
            let stderr = read_ssh_stream_limited(
                &mut stderr_stream,
                REMOTE_SSH_STDERR_LIMIT_BYTES,
                "stderr",
            )
            .map_err(|e| format!("读取标准错误失败: {e}"))?;
            let _ = channel.wait_close();
            let code = channel.exit_status().unwrap_or(0);

            if code == 0 {
                Ok(String::from_utf8_lossy(&stdout).to_string())
            } else {
                let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
                let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
                if !stderr_text.is_empty() {
                    Err(stderr_text)
                } else if !stdout_text.is_empty() {
                    Err(stdout_text)
                } else {
                    Err(format!("SSH 命令失败，exit={code}"))
                }
            }
        })
        .await
        .map_err(|e| format!("命令线程执行失败: {e}"))?;
    }

    let mut cmd = build_ssh_process_command(connection)?;
    let password_auth = is_password_auth(connection);
    cmd.args(build_ssh_args(connection, false, None));
    cmd.arg(remote_command);

    let output = run_command_limited(
        cmd,
        timeout_duration,
        REMOTE_SSH_STDOUT_LIMIT_BYTES,
        REMOTE_SSH_STDERR_LIMIT_BYTES,
        "SSH command",
    )
    .await
    .map_err(|err| map_ssh_process_error("SSH 命令执行失败", err, password_auth))?;

    if password_auth && verification_code.map(str::trim).unwrap_or("").is_empty() {
        let stderr_preview = String::from_utf8_lossy(output.stderr.as_slice()).to_lowercase();
        if stderr_preview.contains("verification code")
            || stderr_preview.contains("one-time")
            || stderr_preview.contains("otp")
            || stderr_preview.contains("验证码")
            || stderr_preview.contains("mfa")
            || stderr_preview.contains("2fa")
        {
            return Err(encode_second_factor_required_error(
                "Verification code / OTP",
            ));
        }
    }

    if output.status.success() {
        return Ok(String::from_utf8_lossy(output.stdout.as_slice()).to_string());
    }

    let stderr = String::from_utf8_lossy(output.stderr.as_slice())
        .trim()
        .to_string();
    if stderr.is_empty() {
        Err(format!("SSH 命令失败，exit={}", output.status))
    } else {
        Err(stderr)
    }
}

fn map_ssh_process_error(prefix: &str, err: BoundedCommandError, password_auth: bool) -> String {
    match err {
        BoundedCommandError::Spawn(err) => map_command_spawn_error(prefix, err, password_auth),
        BoundedCommandError::Timeout => "SSH 命令执行超时".to_string(),
        other => format!("{prefix}: {other}"),
    }
}

fn read_ssh_stream_limited<R>(
    reader: &mut R,
    limit_bytes: usize,
    stream_label: &str,
) -> Result<Vec<u8>, String>
where
    R: Read,
{
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer).map_err(|err| err.to_string())?;
        if read == 0 {
            return Ok(output);
        }
        let next_len = output.len().saturating_add(read);
        ensure_ssh_stream_within_limit(stream_label, next_len, limit_bytes)?;
        output.extend_from_slice(&buffer[..read]);
    }
}

fn ensure_ssh_stream_within_limit(
    stream_label: &str,
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "SSH {stream_label} exceeded output limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::ensure_ssh_stream_within_limit;

    #[test]
    fn ssh_stream_limit_accepts_boundary_size() {
        assert!(ensure_ssh_stream_within_limit("stdout", 1024, 1024).is_ok());
    }

    #[test]
    fn ssh_stream_limit_rejects_oversized_output() {
        let err = ensure_ssh_stream_within_limit("stderr", 1025, 1024)
            .expect_err("oversized output should fail");

        assert!(err.contains("exceeded output limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
