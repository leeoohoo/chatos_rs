use std::cmp::Ordering;
use std::env;
use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path as FsPath;
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use chatos_builtin_tools::{RemoteConnectionControllerContext, RemoteConnectionControllerStore};
use serde::Serialize;
use serde_json::{json, Value};
use ssh2::{
    CheckResult, KeyboardInteractivePrompt, KnownHostFileKind, KnownHostKeyFormat, Prompt, Session,
};
use tokio::task;
use tokio::time::Duration;

use crate::models::{now_rfc3339, RemoteServerRecord, RemoteServerTestResponse};
use crate::store::AppStore;

#[derive(Clone)]
pub struct TaskRunnerRemoteConnectionStore {
    store: AppStore,
}

impl TaskRunnerRemoteConnectionStore {
    pub(crate) fn new(store: AppStore) -> Self {
        Self { store }
    }
}

#[derive(Debug, Serialize)]
struct ConnectionSummary {
    id: String,
    name: String,
    host: String,
    port: i64,
    username: String,
    auth_type: String,
    default_remote_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct RemoteEntry {
    path: String,
    name: String,
    is_dir: bool,
    size: Option<u64>,
    modified_at: Option<String>,
}

#[async_trait]
impl RemoteConnectionControllerStore for TaskRunnerRemoteConnectionStore {
    async fn list_connections(
        &self,
        context: RemoteConnectionControllerContext,
    ) -> Result<Value, String> {
        let mut list = self
            .store
            .list_remote_servers()
            .await?
            .into_iter()
            .filter(|item| item.enabled)
            .collect::<Vec<_>>();
        if let Some(default_connection_id) = context
            .default_remote_connection_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            list.retain(|item| item.id == default_connection_id);
        }
        list.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        let connections = list
            .into_iter()
            .map(|item| ConnectionSummary {
                id: item.id,
                name: item.name,
                host: item.host,
                port: item.port,
                username: item.username,
                auth_type: item.auth_type,
                default_remote_path: item.default_remote_path,
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "count": connections.len(),
            "connections": connections,
        }))
    }

    async fn test_connection(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
    ) -> Result<Value, String> {
        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = self.resolve_enabled_server(&connection_id).await?;
        let response = match test_remote_server_connectivity(&server, Some(server.id.clone())).await
        {
            Ok(response) => {
                self.persist_test_result(&server.id, true, response.remote_host.clone())
                    .await?;
                response
            }
            Err(err) => {
                self.persist_test_result(&server.id, false, Some(err.clone()))
                    .await?;
                return Err(err);
            }
        };
        self.touch_server(&server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "name": server.name,
            "host": server.host,
            "port": server.port,
            "username": server.username,
            "result": {
                "success": response.ok,
                "remote_host": response.remote_host,
                "connected_at": response.tested_at,
            },
        }))
    }

    async fn run_command(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        command: String,
        timeout_seconds: Option<u64>,
        allow_dangerous: bool,
        max_output_chars: Option<usize>,
    ) -> Result<Value, String> {
        if let Some(reason) = command_danger_reason(command.as_str()) {
            if !allow_dangerous {
                return Err(format!(
                    "{reason}。如确实需要执行，请显式设置 allow_dangerous=true"
                ));
            }
        }

        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = self.resolve_enabled_server(&connection_id).await?;
        let timeout = timeout_seconds
            .unwrap_or(context.command_timeout_seconds)
            .clamp(1, context.max_command_timeout_seconds);
        let output_limit = max_output_chars
            .unwrap_or(context.max_output_chars)
            .clamp(128, context.max_output_chars.max(128));

        let output =
            run_ssh_command(&server, command.as_str(), Duration::from_secs(timeout)).await?;
        let (output_text, truncated) = truncate_text(output.as_str(), output_limit);
        self.touch_server(&server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "name": server.name,
            "host": server.host,
            "port": server.port,
            "username": server.username,
            "command": command,
            "timeout_seconds": timeout,
            "output_chars": output_text.chars().count(),
            "output_truncated": truncated,
            "output": output_text,
        }))
    }

    async fn list_directory(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: Option<String>,
        limit: Option<usize>,
    ) -> Result<Value, String> {
        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = self.resolve_enabled_server(&connection_id).await?;
        let normalized_path = normalize_remote_path(
            path.as_deref()
                .filter(|value| !value.trim().is_empty())
                .or(server.default_remote_path.as_deref())
                .unwrap_or("."),
        );
        let entry_limit = limit.unwrap_or(200).clamp(1, 1000);
        let script = format!(
            "set -e; P={quoted}; if [ ! -d \"$P\" ]; then printf '__TASK_RUNNER_DIR_NOT_FOUND__\\n'; else cd \"$P\"; find . -mindepth 1 -maxdepth 1 -printf '%P\\t%y\\t%s\\t%T@\\n'; fi",
            quoted = shell_quote(normalized_path.as_str()),
        );
        let output = run_ssh_command(
            &server,
            script.as_str(),
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        if output.contains("__TASK_RUNNER_DIR_NOT_FOUND__") {
            return Err(format!("远程目录不存在: {normalized_path}"));
        }

        let mut entries = parse_directory_entries(normalized_path.as_str(), output.as_str());
        entries.sort_by(|left, right| match (left.is_dir, right.is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
        });
        let truncated = entries.len() > entry_limit;
        if truncated {
            entries.truncate(entry_limit);
        }
        self.touch_server(&server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "path": normalized_path,
            "count": entries.len(),
            "entries_truncated": truncated,
            "entries": entries,
        }))
    }

    async fn read_file(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: String,
        max_bytes: Option<usize>,
    ) -> Result<Value, String> {
        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = self.resolve_enabled_server(&connection_id).await?;
        let normalized_path = normalize_remote_path(path.as_str());
        let read_limit = max_bytes
            .unwrap_or(context.max_read_file_bytes)
            .clamp(1, context.max_read_file_bytes.max(1));
        let script = format!(
            "set -e; P={quoted}; if [ ! -f \"$P\" ]; then printf '__TASK_RUNNER_FILE_NOT_FOUND__\\n'; else SZ=$(wc -c < \"$P\" 2>/dev/null || echo 0); head -c {limit} \"$P\"; if [ \"$SZ\" -gt {limit} ]; then printf '\\n__TASK_RUNNER_FILE_TRUNCATED__ size=%s limit={limit}\\n' \"$SZ\"; fi; fi",
            quoted = shell_quote(normalized_path.as_str()),
            limit = read_limit,
        );
        let output = run_ssh_command(
            &server,
            script.as_str(),
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        if output.contains("__TASK_RUNNER_FILE_NOT_FOUND__") {
            return Err(format!("远程文件不存在: {normalized_path}"));
        }
        let (content, truncated, source_size) = split_file_output(output);
        self.touch_server(&server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "path": normalized_path,
            "max_bytes": read_limit,
            "source_size_bytes": source_size,
            "truncated": truncated,
            "content": content,
        }))
    }
}

impl TaskRunnerRemoteConnectionStore {
    async fn resolve_enabled_server(
        &self,
        connection_id: &str,
    ) -> Result<RemoteServerRecord, String> {
        let server = self
            .store
            .get_remote_server(connection_id)
            .await?
            .ok_or_else(|| format!("远程服务器不存在: {connection_id}"))?;
        if !server.enabled {
            return Err(format!("远程服务器已禁用: {connection_id}"));
        }
        Ok(server)
    }

    async fn touch_server(&self, connection_id: &str) -> Result<(), String> {
        let Some(mut server) = self.store.get_remote_server(connection_id).await? else {
            return Ok(());
        };
        server.last_active_at = Some(now_rfc3339());
        server.updated_at = now_rfc3339();
        self.store.save_remote_server(server).await?;
        Ok(())
    }

    async fn persist_test_result(
        &self,
        connection_id: &str,
        ok: bool,
        message: Option<String>,
    ) -> Result<(), String> {
        let Some(mut server) = self.store.get_remote_server(connection_id).await? else {
            return Ok(());
        };
        let now = now_rfc3339();
        server.last_tested_at = Some(now.clone());
        server.last_test_status = Some(if ok { "success" } else { "failed" }.to_string());
        server.last_test_message = message;
        server.updated_at = now;
        self.store.save_remote_server(server).await?;
        Ok(())
    }
}

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

async fn run_ssh_command(
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

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    channel
        .read_to_end(&mut stdout)
        .map_err(|err| format!("读取标准输出失败: {err}"))?;
    channel
        .stderr()
        .read_to_end(&mut stderr)
        .map_err(|err| format!("读取标准错误失败: {err}"))?;
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

fn connect_tcp_stream(host: &str, port: i64, timeout: StdDuration) -> Result<TcpStream, String> {
    let addr_text = format!("{host}:{port}");
    let addrs = addr_text
        .to_socket_addrs()
        .map_err(|err| format!("解析远端地址失败: {err}"))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(format!("无法解析远端地址: {addr_text}"));
    }
    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(err) => last_err = Some(err.to_string()),
        }
    }
    Err(format!(
        "连接远端失败: {}",
        last_err.unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn configure_stream_timeout(stream: &TcpStream, timeout: StdDuration) -> Result<(), String> {
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

fn known_hosts_file_path() -> Result<std::path::PathBuf, String> {
    let home = env::var_os("HOME").ok_or_else(|| "无法定位用户 home 目录".to_string())?;
    Ok(std::path::PathBuf::from(home)
        .join(".ssh")
        .join("known_hosts"))
}

fn host_key_format_from_ssh2(key_type: ssh2::HostKeyType) -> Result<KnownHostKeyFormat, String> {
    match key_type {
        ssh2::HostKeyType::Rsa => Ok(KnownHostKeyFormat::SshRsa),
        ssh2::HostKeyType::Dss => Ok(KnownHostKeyFormat::SshDss),
        ssh2::HostKeyType::Ecdsa256 => Ok(KnownHostKeyFormat::Ecdsa256),
        ssh2::HostKeyType::Ecdsa384 => Ok(KnownHostKeyFormat::Ecdsa384),
        ssh2::HostKeyType::Ecdsa521 => Ok(KnownHostKeyFormat::Ecdsa521),
        ssh2::HostKeyType::Ed25519 => Ok(KnownHostKeyFormat::Ed25519),
        ssh2::HostKeyType::Unknown => Err("不支持的主机公钥类型".to_string()),
    }
}

fn host_key_record_name(host: &str, port: i64) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{}]:{}", host, port)
    }
}

fn replace_known_host_entry(
    known_hosts: &mut ssh2::KnownHosts,
    known_hosts_path: &FsPath,
    host: &str,
    port: i64,
    host_key: &[u8],
    host_key_type: ssh2::HostKeyType,
) -> Result<(), String> {
    let mut aliases = vec![host_key_record_name(host, port)];
    if !aliases.iter().any(|item| item == host) {
        aliases.push(host.to_string());
    }

    for entry in known_hosts
        .hosts()
        .map_err(|err| format!("读取 known_hosts 条目失败: {err}"))?
    {
        if let Some(name) = entry.name() {
            if aliases.iter().any(|alias| alias == name) {
                known_hosts
                    .remove(&entry)
                    .map_err(|err| format!("更新 known_hosts 失败: {err}"))?;
            }
        }
    }

    let key_format = host_key_format_from_ssh2(host_key_type)?;
    known_hosts
        .add(
            host_key_record_name(host, port).as_str(),
            host_key,
            "",
            key_format,
        )
        .map_err(|err| format!("写入 known_hosts 失败: {err}"))?;
    known_hosts
        .write_file(known_hosts_path, KnownHostFileKind::OpenSSH)
        .map_err(|err| format!("保存 known_hosts 失败: {err}"))?;
    Ok(())
}

fn apply_host_key_policy(
    session: &Session,
    host: &str,
    port: i64,
    host_key_policy: &str,
) -> Result<(), String> {
    let (host_key, host_key_type) = session
        .host_key()
        .ok_or_else(|| "远端未返回主机公钥".to_string())?;
    let mut known_hosts = session
        .known_hosts()
        .map_err(|err| format!("读取 known_hosts 失败: {err}"))?;
    let known_hosts_path = known_hosts_file_path()?;
    if known_hosts_path.exists() {
        known_hosts
            .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
            .map_err(|err| format!("加载 known_hosts 失败: {err}"))?;
    }

    match known_hosts.check_port(host, port as u16, host_key) {
        CheckResult::Match => Ok(()),
        CheckResult::Mismatch if host_key_policy == "accept_new" => {
            if let Some(parent) = known_hosts_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("创建 ~/.ssh 目录失败: {err}"))?;
            }
            replace_known_host_entry(
                &mut known_hosts,
                &known_hosts_path,
                host,
                port,
                host_key,
                host_key_type,
            )
        }
        CheckResult::Mismatch => Err(
            "主机指纹与 known_hosts 记录不匹配，请核对服务器或切换 accept_new 后重试".to_string(),
        ),
        CheckResult::NotFound if host_key_policy == "accept_new" => {
            if let Some(parent) = known_hosts_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("创建 ~/.ssh 目录失败: {err}"))?;
            }
            replace_known_host_entry(
                &mut known_hosts,
                &known_hosts_path,
                host,
                port,
                host_key,
                host_key_type,
            )
        }
        CheckResult::NotFound => {
            Err("主机指纹未受信任，请先加入 known_hosts 或使用 accept_new".to_string())
        }
        CheckResult::Failure => Err("主机指纹校验失败".to_string()),
    }
}

fn resolve_connection_id(
    context: &RemoteConnectionControllerContext,
    explicit_connection_id: Option<String>,
) -> Result<String, String> {
    if let Some(connection_id) = normalize_optional_string(explicit_connection_id) {
        return Ok(connection_id);
    }
    if let Some(connection_id) =
        normalize_optional_string(context.default_remote_connection_id.clone())
    {
        return Ok(connection_id);
    }
    Err("缺少 connection_id，请先调用 list_connections 选择连接后再重试".to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return ".".to_string();
    }

    let mut normalized = trimmed.replace('\\', "/");
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }

    if normalized.len() > 1 {
        normalized = normalized.trim_end_matches('/').to_string();
    }
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}

fn join_remote_path(base: &str, name: &str) -> String {
    let base = normalize_remote_path(base);
    if base == "." {
        name.to_string()
    } else if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

fn command_danger_reason(command: &str) -> Option<&'static str> {
    let lowered = command.to_lowercase();
    let compact = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.contains("rm -rf /")
        || compact.contains("rm -fr /")
        || compact.contains("rm -rf /*")
        || compact.contains("rm -fr /*")
    {
        return Some("检测到高危删除命令（rm -rf /）");
    }
    if compact.contains("mkfs") {
        return Some("检测到高危磁盘格式化命令（mkfs）");
    }
    if compact.contains("shutdown") || compact.contains("poweroff") || compact.contains("reboot") {
        return Some("检测到高危主机关机/重启命令");
    }
    if compact.contains(":(){:|:&};:") {
        return Some("检测到高危 fork bomb 命令");
    }
    if compact.contains("dd if=") && compact.contains(" of=/dev/") {
        return Some("检测到高危块设备写入命令");
    }
    None
}

fn truncate_text(input: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 {
        return (String::new(), !input.is_empty());
    }
    let total = input.chars().count();
    if total <= max_chars {
        return (input.to_string(), false);
    }
    (input.chars().take(max_chars).collect::<String>(), true)
}

fn parse_directory_entries(base_path: &str, output: &str) -> Vec<RemoteEntry> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let name = parts.next()?.trim();
            if name.is_empty() {
                return None;
            }
            let entry_type = parts.next().unwrap_or("");
            let size = parts.next().and_then(|value| value.parse::<u64>().ok());
            let modified_at = parts
                .next()
                .and_then(|value| value.parse::<f64>().ok())
                .map(|value| chrono::DateTime::<chrono::Utc>::from_timestamp(value as i64, 0))
                .flatten()
                .map(|value| value.to_rfc3339());

            Some(RemoteEntry {
                path: join_remote_path(base_path, name),
                name: name.to_string(),
                is_dir: entry_type == "d",
                size,
                modified_at,
            })
        })
        .collect()
}

fn split_file_output(output: String) -> (String, bool, Option<u64>) {
    let marker = "__TASK_RUNNER_FILE_TRUNCATED__";
    if let Some(index) = output.find(marker) {
        let content = output[..index].trim_end_matches('\n').to_string();
        let metadata = &output[index + marker.len()..];
        let source_size = metadata
            .split_whitespace()
            .find_map(|part| part.strip_prefix("size="))
            .and_then(|value| value.parse::<u64>().ok());
        (content, true, source_size)
    } else {
        (output, false, None)
    }
}
