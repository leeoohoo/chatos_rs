// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io::{Read, Write};
use std::path::Path as FsPath;
use std::time::Duration as StdDuration;

use chatos_remote_runtime::{
    authenticate_private_key_file, configure_stream_timeout, connect_tcp_stream,
    establish_ssh_session, read_stream_limited,
};
use ssh2::{KeyboardInteractivePrompt, OpenFlags, OpenType, Prompt, Session, Sftp};
use tokio::task;
use tokio::time::Duration;

use crate::models::{now_rfc3339, RemoteServerRecord, RemoteServerTestResponse};

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

pub(super) struct SftpDownloadResult {
    pub(super) content: Vec<u8>,
    pub(super) source_size: Option<u64>,
    pub(super) truncated: bool,
}

pub(super) async fn download_sftp_file(
    server: &RemoteServerRecord,
    remote_path: &str,
    max_bytes: usize,
    timeout_duration: Duration,
) -> Result<SftpDownloadResult, String> {
    let server = server.clone();
    let remote_path = remote_path.to_string();
    task::spawn_blocking(move || {
        download_sftp_file_blocking(&server, remote_path.as_str(), max_bytes, timeout_duration)
    })
    .await
    .map_err(|err| format!("SFTP 下载线程执行失败: {err}"))?
}

pub(super) async fn upload_sftp_file(
    server: &RemoteServerRecord,
    remote_path: &str,
    content: Vec<u8>,
    create_parent_dirs: bool,
    overwrite: bool,
    timeout_duration: Duration,
) -> Result<usize, String> {
    let server = server.clone();
    let remote_path = remote_path.to_string();
    task::spawn_blocking(move || {
        upload_sftp_file_blocking(
            &server,
            remote_path.as_str(),
            content.as_slice(),
            create_parent_dirs,
            overwrite,
            timeout_duration,
        )
    })
    .await
    .map_err(|err| format!("SFTP 上传线程执行失败: {err}"))?
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

    let stdout = read_stream_limited(&mut channel, "stdout", SSH_COMMAND_OUTPUT_LIMIT_BYTES)
        .map_err(|error| error.with_read_context("读取 SSH stdout 失败"))?;
    let mut stderr_stream = channel.stderr();
    let stderr = read_stream_limited(&mut stderr_stream, "stderr", SSH_COMMAND_OUTPUT_LIMIT_BYTES)
        .map_err(|error| error.with_read_context("读取 SSH stderr 失败"))?;
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

fn download_sftp_file_blocking(
    server: &RemoteServerRecord,
    remote_path: &str,
    max_bytes: usize,
    timeout_duration: Duration,
) -> Result<SftpDownloadResult, String> {
    let session = connect_ssh_session(server, timeout_duration)?;
    let sftp = session
        .sftp()
        .map_err(|err| format!("初始化 SFTP 失败: {err}"))?;
    let path = FsPath::new(remote_path);
    let source_size = sftp.stat(path).ok().and_then(|stat| stat.size);
    let mut file = sftp
        .open(path)
        .map_err(|err| format!("打开远程文件失败: {err}"))?;
    let mut content = Vec::new();
    let read_limit = max_bytes.saturating_add(1) as u64;
    std::io::Read::by_ref(&mut file)
        .take(read_limit)
        .read_to_end(&mut content)
        .map_err(|err| format!("读取远程文件失败: {err}"))?;

    let read_past_limit = content.len() > max_bytes;
    if read_past_limit {
        content.truncate(max_bytes);
    }
    let truncated = read_past_limit || source_size.is_some_and(|size| size > max_bytes as u64);

    Ok(SftpDownloadResult {
        content,
        source_size,
        truncated,
    })
}

fn upload_sftp_file_blocking(
    server: &RemoteServerRecord,
    remote_path: &str,
    content: &[u8],
    create_parent_dirs: bool,
    overwrite: bool,
    timeout_duration: Duration,
) -> Result<usize, String> {
    let session = connect_ssh_session(server, timeout_duration)?;
    let sftp = session
        .sftp()
        .map_err(|err| format!("初始化 SFTP 失败: {err}"))?;
    let path = FsPath::new(remote_path);

    if create_parent_dirs {
        ensure_sftp_parent_dirs(&sftp, remote_path)?;
    }
    if !overwrite && sftp.stat(path).is_ok() {
        return Err(format!(
            "远程文件已存在: {remote_path}。如需覆盖，请设置 overwrite=true"
        ));
    }

    let mut file = sftp
        .open_mode(
            path,
            OpenFlags::WRITE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .map_err(|err| format!("打开远程写入文件失败: {err}"))?;
    file.write_all(content)
        .map_err(|err| format!("写入远程文件失败: {err}"))?;
    file.flush()
        .map_err(|err| format!("刷新远程文件失败: {err}"))?;
    Ok(content.len())
}

fn ensure_sftp_parent_dirs(sftp: &Sftp, remote_path: &str) -> Result<(), String> {
    let Some(parent) = remote_parent_dir(remote_path) else {
        return Ok(());
    };
    ensure_sftp_dir(sftp, parent.as_str())
}

fn remote_parent_dir(remote_path: &str) -> Option<String> {
    let trimmed = remote_path.trim_end_matches('/');
    let (parent, _) = trimmed.rsplit_once('/')?;
    if parent.is_empty() {
        Some("/".to_string())
    } else {
        Some(parent.to_string())
    }
}

fn ensure_sftp_dir(sftp: &Sftp, dir: &str) -> Result<(), String> {
    let trimmed = dir.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == "/" {
        return Ok(());
    }

    let absolute = trimmed.starts_with('/');
    let mut current = if absolute {
        "/".to_string()
    } else {
        String::new()
    };

    for part in trimmed.split('/').filter(|part| {
        let part = part.trim();
        !part.is_empty() && part != "."
    }) {
        current = if current.is_empty() {
            part.to_string()
        } else if current == "/" {
            format!("/{part}")
        } else {
            format!("{current}/{part}")
        };

        let path = FsPath::new(current.as_str());
        if sftp.stat(path).is_ok() {
            continue;
        }
        if let Err(err) = sftp.mkdir(path, 0o755) {
            if sftp.stat(path).is_err() {
                return Err(format!("创建远程目录失败 {}: {err}", current));
            }
        }
    }
    Ok(())
}

fn connect_ssh_session(
    server: &RemoteServerRecord,
    timeout_duration: Duration,
) -> Result<Session, String> {
    let timeout = StdDuration::from_millis(timeout_duration.as_millis().max(1) as u64);
    let stream = connect_tcp_stream(server.host.as_str(), server.port, timeout)
        .map_err(|error| error.format_tcp_context("远端", " SSH "))?;
    configure_stream_timeout(&stream, timeout)
        .map_err(|error| error.format_tcp_context("远端", " SSH "))?;
    establish_ssh_session(
        stream,
        timeout,
        server.host.as_str(),
        server.port,
        server.host_key_policy.as_str(),
        |session| authenticate_session(session, server),
    )
    .map_err(|error| error.to_string())
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
            authenticate_private_key_file(
                session,
                server.username.as_str(),
                FsPath::new(private_key_path),
                cert_path,
                None,
            )
            .map_err(|err| format!("密钥认证失败: {err}"))?;
        }
        _ => return Err("不支持的认证方式".to_string()),
    }
    Ok(())
}
