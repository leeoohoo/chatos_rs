// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

use chatos_remote_runtime::{
    apply_host_key_policy, authenticate_private_key_file, configure_stream_timeout,
    connect_tcp_stream, establish_ssh_session, read_stream_limited, ssh_timeout_millis,
};
use serde::Deserialize;
use ssh2::{KeyboardInteractivePrompt, Prompt, Session};

const SECOND_FACTOR_REQUIRED_PREFIX: &str = "__CHATOS_SECOND_FACTOR_REQUIRED__:";
const COMMAND_STDOUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const COMMAND_STDERR_LIMIT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RemoteConnectionSpec {
    pub(crate) host: String,
    pub(crate) port: i64,
    pub(crate) username: String,
    pub(crate) auth_type: String,
    pub(crate) password: Option<String>,
    pub(crate) private_key_path: Option<String>,
    pub(crate) certificate_path: Option<String>,
    pub(crate) host_key_policy: String,
    pub(crate) jump_enabled: bool,
    pub(crate) jump_host: Option<String>,
    pub(crate) jump_port: Option<i64>,
    pub(crate) jump_username: Option<String>,
    pub(crate) jump_private_key_path: Option<String>,
    pub(crate) jump_certificate_path: Option<String>,
    pub(crate) jump_password: Option<String>,
}

struct PasswordPrompter {
    password: String,
    verification_code: Option<String>,
    password_used: bool,
    verification_used: bool,
    challenge: Option<String>,
}

impl PasswordPrompter {
    fn new(password: &str, verification_code: Option<&str>) -> Self {
        Self {
            password: password.to_string(),
            verification_code: verification_code
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            password_used: false,
            verification_used: false,
            challenge: None,
        }
    }

    fn note_challenge(&mut self, prompt: &str) {
        if self.challenge.is_none() {
            let prompt = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
            self.challenge = Some(if prompt.is_empty() {
                "请输入验证码 / OTP".to_string()
            } else {
                prompt.chars().take(120).collect()
            });
        }
    }

    fn challenge(&self) -> &str {
        self.challenge.as_deref().unwrap_or("请输入验证码 / OTP")
    }
}

impl KeyboardInteractivePrompt for PasswordPrompter {
    fn prompt<'a>(
        &mut self,
        _username: &str,
        instructions: &str,
        prompts: &[Prompt<'a>],
    ) -> Vec<String> {
        if is_second_factor_prompt(instructions) {
            self.note_challenge(instructions);
        }
        prompts
            .iter()
            .map(|prompt| {
                let text = prompt.text.trim();
                if is_password_prompt(text)
                    || (!self.password_used && !prompt.echo && !is_second_factor_prompt(text))
                {
                    self.password_used = true;
                    return self.password.clone();
                }
                self.note_challenge(text);
                if let Some(code) = self.verification_code.as_ref() {
                    self.verification_used = true;
                    return code.clone();
                }
                String::new()
            })
            .collect()
    }
}

fn is_password_prompt(prompt: &str) -> bool {
    let prompt = prompt.to_lowercase();
    ["password", "passphrase", "密码"]
        .iter()
        .any(|hint| prompt.contains(hint))
}

fn is_second_factor_prompt(prompt: &str) -> bool {
    if is_password_prompt(prompt) {
        return false;
    }
    let prompt = prompt.to_lowercase();
    [
        "otp",
        "passcode",
        "verification code",
        "authentication code",
        "security code",
        "one-time",
        "two-factor",
        "2fa",
        "mfa",
        "token",
        "验证码",
        "动态口令",
        "短信",
    ]
    .iter()
    .any(|hint| prompt.contains(hint))
}

fn authenticate_password(
    session: &Session,
    username: &str,
    password: &str,
    verification_code: Option<&str>,
    label: &str,
) -> Result<(), String> {
    if session.userauth_password(username, password).is_ok() && session.authenticated() {
        return Ok(());
    }
    let mut prompter = PasswordPrompter::new(password, verification_code);
    let result = session.userauth_keyboard_interactive(username, &mut prompter);
    if result.is_ok() && session.authenticated() {
        return Ok(());
    }
    if prompter.challenge.is_some() && !prompter.verification_used {
        return Err(format!(
            "{SECOND_FACTOR_REQUIRED_PREFIX}{}",
            prompter.challenge()
        ));
    }
    if prompter.verification_used {
        return Err(format!("{label}失败: 验证码认证失败或验证码已过期"));
    }
    Err(format!("{label}失败"))
}

fn authenticate_target(
    session: &Session,
    connection: &RemoteConnectionSpec,
    verification_code: Option<&str>,
) -> Result<(), String> {
    match connection.auth_type.as_str() {
        "password" => authenticate_password(
            session,
            connection.username.as_str(),
            connection
                .password
                .as_deref()
                .ok_or_else(|| "password 模式需要提供 password".to_string())?,
            verification_code,
            "密码认证",
        ),
        "private_key" | "private_key_cert" => authenticate_private_key_file(
            session,
            connection.username.as_str(),
            Path::new(
                connection
                    .private_key_path
                    .as_deref()
                    .ok_or_else(|| "私钥路径不能为空".to_string())?,
            ),
            connection.certificate_path.as_deref().map(Path::new),
            None,
        )
        .map_err(|error| format!("密钥认证失败: {error}")),
        _ => Err("不支持的认证方式".to_string()),
    }
}

fn authenticate_jump(
    session: &Session,
    connection: &RemoteConnectionSpec,
    username: &str,
    verification_code: Option<&str>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    if let Some(private_key_path) = connection.jump_private_key_path.as_deref() {
        match authenticate_private_key_file(
            session,
            username,
            Path::new(private_key_path),
            connection.jump_certificate_path.as_deref().map(Path::new),
            None,
        ) {
            Ok(()) => return Ok(()),
            Err(error) => failures.push(format!("跳板机密钥认证失败: {error}")),
        }
    }
    if let Some(password) = connection.jump_password.as_deref() {
        match authenticate_password(
            session,
            username,
            password,
            verification_code,
            "跳板机密码认证",
        ) {
            Ok(()) => return Ok(()),
            Err(error) if extract_second_factor_prompt(error.as_str()).is_some() => {
                return Err(error)
            }
            Err(error) => failures.push(error),
        }
    }
    if connection.auth_type != "password" {
        if let Some(private_key_path) = connection.private_key_path.as_deref() {
            match authenticate_private_key_file(
                session,
                username,
                Path::new(private_key_path),
                connection.certificate_path.as_deref().map(Path::new),
                None,
            ) {
                Ok(()) => return Ok(()),
                Err(error) => failures.push(format!("复用目标密钥认证失败: {error}")),
            }
        }
    }
    if let Some(password) = connection.password.as_deref() {
        match authenticate_password(
            session,
            username,
            password,
            verification_code,
            "复用目标密码认证",
        ) {
            Ok(()) => return Ok(()),
            Err(error) if extract_second_factor_prompt(error.as_str()).is_some() => {
                return Err(error)
            }
            Err(error) => failures.push(error),
        }
    }
    Err(if failures.is_empty() {
        "跳板机认证失败".to_string()
    } else {
        failures.join("；")
    })
}

pub(super) fn is_would_block(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock
}

pub(super) fn is_ssh_would_block(error: &ssh2::Error) -> bool {
    matches!(error.code(), ssh2::ErrorCode::Session(-37))
}

fn forward_jump_tunnel(
    local_stream: &mut TcpStream,
    jump_channel: &mut ssh2::Channel,
) -> Result<(), String> {
    const BUFFER_SIZE: usize = 8192;
    const MAX_PENDING: usize = 256 * 1024;
    let mut from_local = [0u8; BUFFER_SIZE];
    let mut from_remote = [0u8; BUFFER_SIZE];
    let mut pending_to_remote = Vec::new();
    let mut pending_to_local = Vec::new();
    let mut local_eof = false;
    let mut remote_eof = false;
    let mut remote_eof_sent = false;
    let mut local_shutdown = false;

    loop {
        let mut progressed = false;
        if !local_eof && pending_to_remote.len() < MAX_PENDING {
            match local_stream.read(&mut from_local) {
                Ok(0) => {
                    local_eof = true;
                    progressed = true;
                }
                Ok(size) => {
                    pending_to_remote.extend_from_slice(&from_local[..size]);
                    progressed = true;
                }
                Err(error) if is_would_block(&error) => {}
                Err(error) => return Err(format!("读取本地隧道失败: {error}")),
            }
        }
        while !pending_to_remote.is_empty() {
            match jump_channel.write(pending_to_remote.as_slice()) {
                Ok(0) => return Err("跳板机隧道已关闭".to_string()),
                Ok(size) => {
                    pending_to_remote.drain(..size);
                    progressed = true;
                }
                Err(error) if is_would_block(&error) => break,
                Err(error) => return Err(format!("写入跳板机隧道失败: {error}")),
            }
        }
        if !remote_eof && pending_to_local.len() < MAX_PENDING {
            match jump_channel.read(&mut from_remote) {
                Ok(0) if jump_channel.eof() => {
                    remote_eof = true;
                    progressed = true;
                }
                Ok(0) => {}
                Ok(size) => {
                    pending_to_local.extend_from_slice(&from_remote[..size]);
                    progressed = true;
                }
                Err(error) if is_would_block(&error) => {}
                Err(error) => return Err(format!("读取跳板机隧道失败: {error}")),
            }
        }
        while !pending_to_local.is_empty() {
            match local_stream.write(pending_to_local.as_slice()) {
                Ok(0) => return Err("本地隧道已关闭".to_string()),
                Ok(size) => {
                    pending_to_local.drain(..size);
                    progressed = true;
                }
                Err(error) if is_would_block(&error) => break,
                Err(error) => return Err(format!("写入本地隧道失败: {error}")),
            }
        }
        if local_eof && pending_to_remote.is_empty() && !remote_eof_sent {
            match jump_channel.send_eof() {
                Ok(()) => remote_eof_sent = true,
                Err(error) if is_ssh_would_block(&error) => {}
                Err(error) => return Err(format!("关闭跳板机发送流失败: {error}")),
            }
        }
        if remote_eof && pending_to_local.is_empty() && !local_shutdown {
            let _ = local_stream.shutdown(Shutdown::Write);
            local_shutdown = true;
        }
        if local_eof && remote_eof && pending_to_remote.is_empty() && pending_to_local.is_empty() {
            let _ = jump_channel.close();
            let _ = jump_channel.wait_close();
            return Ok(());
        }
        if !progressed {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn create_jump_tunnel(
    connection: &RemoteConnectionSpec,
    timeout: Duration,
    verification_code: Option<&str>,
) -> Result<TcpStream, String> {
    let jump_host = connection
        .jump_host
        .as_deref()
        .ok_or_else(|| "启用跳板机时 jump_host 不能为空".to_string())?;
    let jump_username = connection
        .jump_username
        .as_deref()
        .ok_or_else(|| "启用跳板机时 jump_username 不能为空".to_string())?;
    let jump_port = connection.jump_port.unwrap_or(22);
    let jump_stream = connect_tcp_stream(jump_host, jump_port, timeout)
        .map_err(|error| error.format_tcp_context("跳板机", "跳板机"))?;
    configure_stream_timeout(&jump_stream, timeout)
        .map_err(|error| error.format_tcp_context("跳板机", "跳板机"))?;
    let mut jump_session = Session::new().map_err(|error| error.to_string())?;
    jump_session.set_tcp_stream(jump_stream);
    jump_session.set_timeout(ssh_timeout_millis(timeout));
    jump_session
        .handshake()
        .map_err(|error| format!("跳板机 SSH 握手失败: {error}"))?;
    apply_host_key_policy(
        &jump_session,
        jump_host,
        jump_port,
        connection.host_key_policy.as_str(),
    )?;
    authenticate_jump(&jump_session, connection, jump_username, verification_code)?;
    let target_port = u16::try_from(connection.port).map_err(|_| "目标端口无效".to_string())?;
    let jump_channel = jump_session
        .channel_direct_tcpip(connection.host.as_str(), target_port, None)
        .map_err(|error| format!("建立跳板机转发通道失败: {error}"))?;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("创建本地跳板通道失败: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("设置本地跳板通道失败: {error}"))?;
    let local_addr = listener.local_addr().map_err(|error| error.to_string())?;
    std::thread::spawn(move || {
        let deadline = Instant::now() + timeout;
        let mut local_stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if is_would_block(&error) && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return,
            }
        };
        let _ = local_stream.set_nonblocking(true);
        jump_session.set_blocking(false);
        let mut jump_channel = jump_channel;
        let _ = forward_jump_tunnel(&mut local_stream, &mut jump_channel);
    });
    let local_stream = TcpStream::connect_timeout(&local_addr, timeout)
        .map_err(|error| format!("连接本地跳板通道失败: {error}"))?;
    configure_stream_timeout(&local_stream, timeout)
        .map_err(|error| error.format_tcp_context("本地跳板通道", "本地跳板通道"))?;
    Ok(local_stream)
}

pub(super) fn connect_session(
    connection: &RemoteConnectionSpec,
    timeout: Duration,
    verification_code: Option<&str>,
) -> Result<Session, String> {
    let stream = if connection.jump_enabled {
        create_jump_tunnel(connection, timeout, verification_code)?
    } else {
        let stream = connect_tcp_stream(connection.host.as_str(), connection.port, timeout)
            .map_err(|error| error.format_tcp_context("远端", "远端"))?;
        configure_stream_timeout(&stream, timeout)
            .map_err(|error| error.format_tcp_context("远端", "远端"))?;
        stream
    };
    establish_ssh_session(
        stream,
        timeout,
        connection.host.as_str(),
        connection.port,
        connection.host_key_policy.as_str(),
        |session| authenticate_target(session, connection, verification_code),
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn run_command(
    connection: &RemoteConnectionSpec,
    command: &str,
    timeout: Duration,
    verification_code: Option<&str>,
) -> Result<String, String> {
    let session = connect_session(connection, timeout, verification_code)?;
    let mut channel = session
        .channel_session()
        .map_err(|error| format!("创建命令通道失败: {error}"))?;
    channel
        .exec(command)
        .map_err(|error| format!("执行远端命令失败: {error}"))?;
    let stdout = read_stream_limited(&mut channel, "stdout", COMMAND_STDOUT_LIMIT_BYTES)
        .map_err(|error| format!("读取标准输出失败: {error}"))?;
    let stderr = read_stream_limited(&mut channel.stderr(), "stderr", COMMAND_STDERR_LIMIT_BYTES)
        .map_err(|error| format!("读取标准错误失败: {error}"))?;
    let _ = channel.wait_close();
    let code = channel.exit_status().unwrap_or(0);
    if code == 0 {
        Ok(String::from_utf8_lossy(stdout.as_slice()).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(stderr.as_slice())
            .trim()
            .to_string();
        if stderr.is_empty() {
            Err(format!("SSH 命令失败，exit={code}"))
        } else {
            Err(stderr)
        }
    }
}

pub(crate) fn test_connectivity(
    connection: &RemoteConnectionSpec,
    verification_code: Option<&str>,
) -> Result<String, String> {
    let output = run_command(
        connection,
        "printf '__CHATOS_OK__\\n'; uname -n 2>/dev/null || hostname",
        Duration::from_secs(12),
        verification_code,
    )?;
    if !output.contains("__CHATOS_OK__") {
        return Err("远端未返回预期握手标识".to_string());
    }
    Ok(output
        .lines()
        .filter(|line| !line.contains("__CHATOS_OK__"))
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| connection.host.clone()))
}

pub(crate) fn extract_second_factor_prompt(error: &str) -> Option<String> {
    let index = error.find(SECOND_FACTOR_REQUIRED_PREFIX)?;
    error[index + SECOND_FACTOR_REQUIRED_PREFIX.len()..]
        .split(['；', ';', '。', '\n', '\r'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_second_factor_challenge() {
        assert_eq!(
            extract_second_factor_prompt("auth: __CHATOS_SECOND_FACTOR_REQUIRED__:SMS code"),
            Some("SMS code".to_string())
        );
    }
}
