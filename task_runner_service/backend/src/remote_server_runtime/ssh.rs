use std::io::Read;
use std::path::Path as FsPath;
use std::time::Duration as StdDuration;

use ssh2::{KeyboardInteractivePrompt, Prompt, Session};
use tokio::task;
use tokio::time::Duration;

use crate::models::{now_rfc3339, RemoteServerRecord, RemoteServerTestResponse};

use super::host_keys::apply_host_key_policy;

const SSH_COMMAND_OUTPUT_LIMIT_BYTES: usize = 512 * 1024;

pub async fn test_remote_server_connectivity(
    server: &RemoteServerRecord,
    server_id: Option<String>,
) -> Result<RemoteServerTestResponse, String> {
    let output = run_ssh_command(
        server,
        "printf '__TASK_RUNNER_OK__\\n'; uname -n 2>/dev/null || hostname",
        Duration::from_secs(12),
    )
    .await?;
    if !output.contains("__TASK_RUNNER_OK__") {
        return Err("远端未返回预期握手标识".to_string());
    }
    let remote_host = output
        .lines()
        .filter(|line| !line.contains("__TASK_RUNNER_OK__"))
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| server.host.clone());
    Ok(RemoteServerTestResponse {
        ok: true,
        server_id,
        name: server.name.clone(),
        host: server.host.clone(),
        port: server.port,
        username: server.username.clone(),
        auth_type: server.auth_type.clone(),
        remote_host: Some(remote_host),
        error: None,
        tested_at: now_rfc3339(),
    })
}

struct PasswordPrompter {
    password: String,
}

impl KeyboardInteractivePrompt for PasswordPrompter {
    fn prompt<'a>(
        &mut self,
        _username: &str,
        _instructions: &str,
        prompts: &[Prompt<'a>],
    ) -> Vec<String> {
        prompts
            .iter()
            .map(|prompt| {
                if prompt.echo {
                    String::new()
                } else {
                    self.password.clone()
                }
            })
            .collect()
    }
}

pub(super) async fn run_ssh_command(
    server: &RemoteServerRecord,
    remote_command: &str,
    timeout_duration: Duration,
) -> Result<String, String> {
    let server = server.clone();
    let remote_command = remote_command.to_string();
    task::spawn_blocking(move || {
        run_ssh_command_blocking(&server, remote_command.as_str(), timeout_duration)
    })
    .await
    .map_err(|err| format!("SSH 命令线程执行失败: {err}"))?
}

fn run_ssh_command_blocking(
    server: &RemoteServerRecord,
    remote_command: &str,
    timeout_duration: Duration,
) -> Result<String, String> {
    let session = connect_ssh_session(server, timeout_duration)?;
    let mut channel = session
        .channel_session()
        .map_err(|err| format!("创建命令通道失败: {err}"))?;
    channel
        .exec(remote_command)
        .map_err(|err| format!("执行远端命令失败: {err}"))?;

    let stdout = read_ssh_stream_limited(&mut channel, "stdout")?;
    let mut stderr_stream = channel.stderr();
    let stderr = read_ssh_stream_limited(&mut stderr_stream, "stderr")?;
    let _ = channel.wait_close();
    let exit_code = channel.exit_status().unwrap_or(0);
    if exit_code == 0 {
        Ok(String::from_utf8_lossy(&stdout).to_string())
    } else {
        let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
        let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
        if !stderr_text.is_empty() {
            Err(stderr_text)
        } else if !stdout_text.is_empty() {
            Err(stdout_text)
        } else {
            Err(format!("SSH 命令失败，exit={exit_code}"))
        }
    }
}

fn read_ssh_stream_limited<R: Read>(reader: &mut R, stream_label: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buffer = [0u8; 8 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| format!("读取 SSH {stream_label} 失败: {err}"))?;
        if read == 0 {
            return Ok(out);
        }

        let next_len = out.len().saturating_add(read);
        ensure_ssh_output_within_limit(stream_label, next_len)?;
        out.extend_from_slice(&buffer[..read]);
    }
}

fn ensure_ssh_output_within_limit(stream_label: &str, actual_bytes: usize) -> Result<(), String> {
    if actual_bytes > SSH_COMMAND_OUTPUT_LIMIT_BYTES {
        return Err(format!(
            "SSH {stream_label} exceeded output limit: {actual_bytes} bytes > {SSH_COMMAND_OUTPUT_LIMIT_BYTES} bytes"
        ));
    }
    Ok(())
}

fn connect_ssh_session(
    server: &RemoteServerRecord,
    timeout_duration: Duration,
) -> Result<Session, String> {
    let timeout = StdDuration::from_millis(timeout_duration.as_millis().max(1) as u64);
    let timeout_ms = timeout_duration.as_millis().clamp(1_000, u32::MAX as u128) as u32;
    let stream = connect_tcp_stream(server.host.as_str(), server.port, timeout)?;
    configure_stream_timeout(&stream, timeout)?;

    let mut session = Session::new().map_err(|err| format!("创建 SSH 会话失败: {err}"))?;
    session.set_tcp_stream(stream);
    session.set_timeout(timeout_ms);
    session
        .handshake()
        .map_err(|err| format!("SSH 握手失败: {err}"))?;
    apply_host_key_policy(
        &session,
        server.host.as_str(),
        server.port,
        server.host_key_policy.as_str(),
    )?;
    authenticate_session(&session, server)?;
    if !session.authenticated() {
        return Err("SSH 认证失败".to_string());
    }
    Ok(session)
}

fn connect_tcp_stream(
    host: &str,
    port: i64,
    timeout: StdDuration,
) -> Result<std::net::TcpStream, String> {
    let addr_text = format!("{host}:{port}");
    let addrs = std::net::ToSocketAddrs::to_socket_addrs(&addr_text)
        .map_err(|err| format!("解析远端地址失败: {err}"))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(format!("无法解析远端地址: {addr_text}"));
    }
    let mut last_err = None;
    for addr in addrs {
        match std::net::TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(err) => last_err = Some(err.to_string()),
        }
    }
    Err(format!(
        "连接远端失败: {}",
        last_err.unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn configure_stream_timeout(
    stream: &std::net::TcpStream,
    timeout: StdDuration,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| format!("设置 SSH 读超时失败: {err}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|err| format!("设置 SSH 写超时失败: {err}"))?;
    Ok(())
}

fn authenticate_session(session: &Session, server: &RemoteServerRecord) -> Result<(), String> {
    match server.auth_type.as_str() {
        "password" => {
            let password = server
                .password
                .as_deref()
                .ok_or_else(|| "password 模式需要提供 password".to_string())?;
            if session
                .userauth_password(server.username.as_str(), password)
                .is_ok()
                && session.authenticated()
            {
                return Ok(());
            }
            let mut prompter = PasswordPrompter {
                password: password.to_string(),
            };
            session
                .userauth_keyboard_interactive(server.username.as_str(), &mut prompter)
                .map_err(|err| format!("密码认证失败: {err}"))?;
        }
        "private_key" | "private_key_cert" => {
            let private_key_path = server
                .private_key_path
                .as_ref()
                .ok_or_else(|| "私钥路径不能为空".to_string())?;
            let cert_path = server.certificate_path.as_deref().map(FsPath::new);
            session
                .userauth_pubkey_file(
                    server.username.as_str(),
                    cert_path,
                    FsPath::new(private_key_path),
                    None,
                )
                .map_err(|err| format!("密钥认证失败: {err}"))?;
        }
        _ => return Err("不支持的认证方式".to_string()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_ssh_output_within_limit, SSH_COMMAND_OUTPUT_LIMIT_BYTES};

    #[test]
    fn ssh_output_limit_accepts_boundary_size() {
        assert!(ensure_ssh_output_within_limit("stdout", SSH_COMMAND_OUTPUT_LIMIT_BYTES).is_ok());
    }

    #[test]
    fn ssh_output_limit_rejects_oversized_output() {
        let err = ensure_ssh_output_within_limit("stderr", SSH_COMMAND_OUTPUT_LIMIT_BYTES + 1)
            .expect_err("oversized output should fail");

        assert!(err.contains("SSH stderr exceeded output limit"));
    }
}
