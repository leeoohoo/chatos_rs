use crate::models::remote_connection::RemoteConnection;
use ssh2::{OpenFlags, OpenType};
use std::fmt::{Display, Formatter};
use std::io::{Read, Write};
use std::path::{Path as FsPath, PathBuf};
use std::process::Stdio;
use tokio::time::{timeout, Duration};
use walkdir::WalkDir;

use super::{
    build_scp_args, build_scp_process_command, connect_ssh2_session, is_password_auth,
    join_remote_path, map_command_spawn_error, normalize_remote_path, remote_parent_path,
    shell_quote, should_use_native_ssh, SftpTransferManager,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteTransferErrorCode {
    AuthFailed,
    PathNotFound,
    PermissionDenied,
    NetworkDisconnected,
    Protocol,
}

impl RemoteTransferErrorCode {
    pub(super) fn as_api_code(self) -> &'static str {
        match self {
            Self::AuthFailed => "remote_auth_failed",
            Self::PathNotFound => "remote_path_not_found",
            Self::PermissionDenied => "remote_permission_denied",
            Self::NetworkDisconnected => "remote_network_disconnected",
            Self::Protocol => "remote_error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TransferJobError {
    Cancelled,
    Timeout(String),
    Io(String),
    Remote {
        code: RemoteTransferErrorCode,
        message: String,
    },
    Message(String),
}

impl TransferJobError {
    fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    fn timeout(message: impl Into<String>) -> Self {
        Self::Timeout(message.into())
    }

    fn io(message: impl Into<String>) -> Self {
        Self::Io(message.into())
    }

    fn remote(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::Remote {
            code: classify_remote_transfer_error_code(message.as_str()),
            message,
        }
    }

    pub(super) fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

impl Display for TransferJobError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "传输已取消"),
            Self::Timeout(message) => write!(f, "{message}"),
            Self::Io(message) => write!(f, "{message}"),
            Self::Remote { message, .. } => write!(f, "{message}"),
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

fn classify_remote_transfer_error_code(message: &str) -> RemoteTransferErrorCode {
    let normalized = message.to_lowercase();
    if normalized.contains("authentication")
        || normalized.contains("auth fail")
        || normalized.contains("ssh 认证失败")
        || normalized.contains("permission denied (publickey")
        || normalized.contains("permission denied, please try again")
    {
        return RemoteTransferErrorCode::AuthFailed;
    }
    if normalized.contains("no such file")
        || normalized.contains("not found")
        || normalized.contains("路径不存在")
    {
        return RemoteTransferErrorCode::PathNotFound;
    }
    if normalized.contains("permission denied") || normalized.contains("权限不足") {
        return RemoteTransferErrorCode::PermissionDenied;
    }
    if normalized.contains("connection reset")
        || normalized.contains("broken pipe")
        || normalized.contains("connection closed")
        || normalized.contains("connection timed out")
        || normalized.contains("timed out")
        || normalized.contains("network is unreachable")
        || normalized.contains("no route to host")
        || normalized.contains("网络中断")
    {
        return RemoteTransferErrorCode::NetworkDisconnected;
    }
    RemoteTransferErrorCode::Protocol
}

fn check_transfer_not_cancelled(
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), TransferJobError> {
    if transfer_manager.is_cancel_requested(transfer_id) {
        return Err(TransferJobError::Cancelled);
    }
    Ok(())
}

pub(super) fn estimate_local_total_bytes_typed(path: &FsPath) -> Result<u64, TransferJobError> {
    if path.is_file() {
        return path
            .metadata()
            .map(|meta| meta.len())
            .map_err(|e| TransferJobError::io(format!("读取本地文件信息失败: {e}")));
    }
    if path.is_dir() {
        let mut total: u64 = 0;
        for entry in WalkDir::new(path) {
            let entry =
                entry.map_err(|e| TransferJobError::io(format!("扫描本地目录失败: {e}")))?;
            if entry.file_type().is_file() {
                total = total.saturating_add(
                    entry
                        .metadata()
                        .map_err(|e| TransferJobError::io(format!("读取本地文件信息失败: {e}")))?
                        .len(),
                );
            }
        }
        return Ok(total);
    }
    Err(TransferJobError::message("本地路径必须是文件或目录"))
}

fn remote_pathbuf_to_string(path: &PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn ensure_remote_dir_recursive(sftp: &ssh2::Sftp, dir_path: &str) -> Result<(), TransferJobError> {
    let normalized = normalize_remote_path(dir_path);
    if normalized == "." || normalized == "/" {
        return Ok(());
    }

    let is_absolute = normalized.starts_with('/');
    let mut current = String::new();
    if is_absolute {
        current.push('/');
    }

    for segment in normalized
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != ".")
    {
        if !current.is_empty() && !current.ends_with('/') {
            current.push('/');
        }
        current.push_str(segment);

        let current_path = FsPath::new(current.as_str());
        match sftp.stat(current_path) {
            Ok(stat) => {
                if !stat.is_dir() {
                    return Err(TransferJobError::remote(format!(
                        "远端路径不是目录: {}",
                        current
                    )));
                }
            }
            Err(_) => {
                if let Err(err) = sftp.mkdir(current_path, 0o755) {
                    match sftp.stat(current_path) {
                        Ok(stat) if stat.is_dir() => {}
                        _ => {
                            return Err(TransferJobError::remote(format!(
                                "创建远端目录失败 ({}): {err}",
                                current
                            )))
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn copy_local_file_to_remote_with_progress(
    sftp: &ssh2::Sftp,
    local_path: &FsPath,
    remote_path: &str,
    total_bytes: u64,
    transferred_bytes: &mut u64,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), TransferJobError> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if let Some(parent) = remote_parent_path(remote_path) {
        ensure_remote_dir_recursive(sftp, parent.as_str())?;
    }

    let mut local_file = std::fs::File::open(local_path)
        .map_err(|e| TransferJobError::io(format!("读取本地文件失败: {e}")))?;
    let mut remote_file = sftp
        .open_mode(
            FsPath::new(remote_path),
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .map_err(|e| TransferJobError::remote(format!("打开远端文件失败: {e}")))?;

    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_transfer_not_cancelled(transfer_id, transfer_manager)?;
        let n = local_file
            .read(&mut buffer)
            .map_err(|e| TransferJobError::io(format!("读取本地文件失败: {e}")))?;
        if n == 0 {
            break;
        }
        remote_file
            .write_all(&buffer[..n])
            .map_err(|e| TransferJobError::remote(format!("写入远端文件失败: {e}")))?;
        *transferred_bytes = transferred_bytes.saturating_add(n as u64);
        transfer_manager.set_progress(
            transfer_id,
            *transferred_bytes,
            Some(total_bytes),
            Some(local_path.to_string_lossy().to_string()),
        );
    }

    Ok(())
}

fn upload_path_recursive_with_progress(
    sftp: &ssh2::Sftp,
    local_path: &FsPath,
    remote_path: &str,
    total_bytes: u64,
    transferred_bytes: &mut u64,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), TransferJobError> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if local_path.is_file() {
        return copy_local_file_to_remote_with_progress(
            sftp,
            local_path,
            remote_path,
            total_bytes,
            transferred_bytes,
            transfer_id,
            transfer_manager,
        );
    }

    if local_path.is_dir() {
        ensure_remote_dir_recursive(sftp, remote_path)?;
        let entries = std::fs::read_dir(local_path)
            .map_err(|e| TransferJobError::io(format!("读取本地目录失败: {e}")))?;
        for entry in entries {
            check_transfer_not_cancelled(transfer_id, transfer_manager)?;
            let entry =
                entry.map_err(|e| TransferJobError::io(format!("读取本地目录失败: {e}")))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let child_local = entry.path();
            let child_remote = join_remote_path(remote_path, name.as_str());
            upload_path_recursive_with_progress(
                sftp,
                child_local.as_path(),
                child_remote.as_str(),
                total_bytes,
                transferred_bytes,
                transfer_id,
                transfer_manager,
            )?;
        }
        return Ok(());
    }

    Err(TransferJobError::message("本地路径必须是文件或目录"))
}

fn compute_remote_total_bytes_with_stat(
    sftp: &ssh2::Sftp,
    remote_path: &str,
    stat: &ssh2::FileStat,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<u64, TransferJobError> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if stat.is_file() {
        return Ok(stat.size.unwrap_or(0));
    }
    if stat.is_dir() {
        let mut total = 0u64;
        let entries = sftp
            .readdir(FsPath::new(remote_path))
            .map_err(|e| TransferJobError::remote(format!("读取远端目录失败: {e}")))?;
        for (entry_path, entry_stat) in entries {
            check_transfer_not_cancelled(transfer_id, transfer_manager)?;
            let child_remote = remote_pathbuf_to_string(&entry_path);
            total = total.saturating_add(compute_remote_total_bytes_with_stat(
                sftp,
                child_remote.as_str(),
                &entry_stat,
                transfer_id,
                transfer_manager,
            )?);
        }
        return Ok(total);
    }
    Ok(0)
}

fn copy_remote_file_to_local_with_progress(
    sftp: &ssh2::Sftp,
    remote_path: &str,
    local_path: &FsPath,
    total_bytes: u64,
    transferred_bytes: &mut u64,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), TransferJobError> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| TransferJobError::io(format!("创建本地目录失败: {e}")))?;
    }

    let mut remote_file = sftp
        .open(FsPath::new(remote_path))
        .map_err(|e| TransferJobError::remote(format!("读取远端文件失败: {e}")))?;
    let mut local_file = std::fs::File::create(local_path)
        .map_err(|e| TransferJobError::io(format!("创建本地文件失败: {e}")))?;

    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_transfer_not_cancelled(transfer_id, transfer_manager)?;
        let n = remote_file
            .read(&mut buffer)
            .map_err(|e| TransferJobError::remote(format!("读取远端文件失败: {e}")))?;
        if n == 0 {
            break;
        }
        local_file
            .write_all(&buffer[..n])
            .map_err(|e| TransferJobError::io(format!("写入本地文件失败: {e}")))?;
        *transferred_bytes = transferred_bytes.saturating_add(n as u64);
        transfer_manager.set_progress(
            transfer_id,
            *transferred_bytes,
            Some(total_bytes),
            Some(remote_path.to_string()),
        );
    }

    Ok(())
}

fn download_remote_path_with_progress(
    sftp: &ssh2::Sftp,
    remote_path: &str,
    remote_stat: &ssh2::FileStat,
    local_path: &FsPath,
    total_bytes: u64,
    transferred_bytes: &mut u64,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), TransferJobError> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if remote_stat.is_file() {
        return copy_remote_file_to_local_with_progress(
            sftp,
            remote_path,
            local_path,
            total_bytes,
            transferred_bytes,
            transfer_id,
            transfer_manager,
        );
    }

    if remote_stat.is_dir() {
        if local_path.exists() && !local_path.is_dir() {
            return Err(TransferJobError::message("本地目标已存在且不是目录"));
        }
        std::fs::create_dir_all(local_path)
            .map_err(|e| TransferJobError::io(format!("创建本地目录失败: {e}")))?;
        let entries = sftp
            .readdir(FsPath::new(remote_path))
            .map_err(|e| TransferJobError::remote(format!("读取远端目录失败: {e}")))?;

        for (entry_path, entry_stat) in entries {
            check_transfer_not_cancelled(transfer_id, transfer_manager)?;
            let entry_name = match entry_path.file_name().and_then(|v| v.to_str()) {
                Some(v) => v.to_string(),
                None => continue,
            };
            let child_remote = remote_pathbuf_to_string(&entry_path);
            let child_local = local_path.join(entry_name);
            download_remote_path_with_progress(
                sftp,
                child_remote.as_str(),
                &entry_stat,
                child_local.as_path(),
                total_bytes,
                transferred_bytes,
                transfer_id,
                transfer_manager,
            )?;
        }
        return Ok(());
    }

    Err(TransferJobError::remote("远端路径既不是文件也不是目录"))
}

pub(super) fn run_sftp_transfer_job_typed(
    connection: &RemoteConnection,
    transfer_id: &str,
    direction: &str,
    local_path: &str,
    remote_path: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<String, TransferJobError> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    let connected = connect_ssh2_session(connection, Duration::from_secs(20))
        .map_err(TransferJobError::remote)?;
    let sftp = connected
        .session
        .sftp()
        .map_err(|e| TransferJobError::remote(format!("初始化 SFTP 失败: {e}")))?;

    if direction == "upload" {
        let source = FsPath::new(local_path);
        if !source.exists() {
            return Err(TransferJobError::message("本地路径不存在"));
        }
        let total_bytes = estimate_local_total_bytes_typed(source)?;
        let mut transferred_bytes = 0u64;
        transfer_manager.set_progress(
            transfer_id,
            0,
            Some(total_bytes),
            Some(local_path.to_string()),
        );
        upload_path_recursive_with_progress(
            &sftp,
            source,
            remote_path,
            total_bytes,
            &mut transferred_bytes,
            transfer_id,
            transfer_manager,
        )?;
        check_transfer_not_cancelled(transfer_id, transfer_manager)?;
        let summary = if source.is_dir() {
            "目录上传完成".to_string()
        } else {
            "文件上传完成".to_string()
        };
        return Ok(summary);
    }

    if direction == "download" {
        let remote_stat = sftp
            .stat(FsPath::new(remote_path))
            .map_err(|e| TransferJobError::remote(format!("读取远端路径信息失败: {e}")))?;
        let total_bytes = compute_remote_total_bytes_with_stat(
            &sftp,
            remote_path,
            &remote_stat,
            transfer_id,
            transfer_manager,
        )?;
        let mut transferred_bytes = 0u64;
        transfer_manager.set_progress(
            transfer_id,
            0,
            Some(total_bytes),
            Some(remote_path.to_string()),
        );
        download_remote_path_with_progress(
            &sftp,
            remote_path,
            &remote_stat,
            FsPath::new(local_path),
            total_bytes,
            &mut transferred_bytes,
            transfer_id,
            transfer_manager,
        )?;
        check_transfer_not_cancelled(transfer_id, transfer_manager)?;
        let summary = if remote_stat.is_dir() {
            "目录下载完成".to_string()
        } else {
            "文件下载完成".to_string()
        };
        return Ok(summary);
    }

    Err(TransferJobError::message(
        "direction 仅支持 upload 或 download",
    ))
}

pub(super) async fn run_scp_upload_typed(
    connection: &RemoteConnection,
    local_path: &str,
    remote_path: &str,
) -> Result<(), TransferJobError> {
    if should_use_native_ssh(connection) {
        let connection = connection.clone();
        let local = local_path.to_string();
        let remote = remote_path.to_string();
        return tokio::task::spawn_blocking(move || {
            let connected = connect_ssh2_session(&connection, Duration::from_secs(15))
                .map_err(TransferJobError::remote)?;
            let sftp = connected
                .session
                .sftp()
                .map_err(|e| TransferJobError::remote(format!("初始化 SFTP 失败: {e}")))?;
            let mut local_file = std::fs::File::open(local.as_str())
                .map_err(|e| TransferJobError::io(format!("读取本地文件失败: {e}")))?;
            let mut remote_file = sftp
                .open_mode(
                    FsPath::new(remote.as_str()),
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                    0o644,
                    OpenType::File,
                )
                .map_err(|e| TransferJobError::remote(format!("打开远端文件失败: {e}")))?;
            std::io::copy(&mut local_file, &mut remote_file)
                .map_err(|e| TransferJobError::remote(format!("上传文件失败: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| TransferJobError::message(format!("上传线程执行失败: {e}")))?;
    }

    let mut cmd = build_scp_process_command(connection).map_err(TransferJobError::message)?;
    let password_auth = is_password_auth(connection);
    cmd.args(build_scp_args(connection));
    cmd.arg(local_path);
    cmd.arg(format!(
        "{}@{}:{}",
        connection.username,
        connection.host,
        shell_quote(remote_path)
    ));
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = timeout(Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| TransferJobError::timeout("上传超时"))?
        .map_err(|e| {
            TransferJobError::remote(map_command_spawn_error("上传失败", e, password_auth))
        })?;

    if output.status.success() {
        return Ok(());
    }

    Err(TransferJobError::remote(
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

pub(super) async fn run_scp_download_typed(
    connection: &RemoteConnection,
    remote_path: &str,
    local_path: &str,
) -> Result<(), TransferJobError> {
    if should_use_native_ssh(connection) {
        let connection = connection.clone();
        let local = local_path.to_string();
        let remote = remote_path.to_string();
        return tokio::task::spawn_blocking(move || {
            let connected = connect_ssh2_session(&connection, Duration::from_secs(15))
                .map_err(TransferJobError::remote)?;
            let sftp = connected
                .session
                .sftp()
                .map_err(|e| TransferJobError::remote(format!("初始化 SFTP 失败: {e}")))?;
            let mut remote_file = sftp
                .open(FsPath::new(remote.as_str()))
                .map_err(|e| TransferJobError::remote(format!("读取远端文件失败: {e}")))?;
            let mut local_file = std::fs::File::create(local.as_str())
                .map_err(|e| TransferJobError::io(format!("创建本地文件失败: {e}")))?;
            std::io::copy(&mut remote_file, &mut local_file)
                .map_err(|e| TransferJobError::remote(format!("下载文件失败: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| TransferJobError::message(format!("下载线程执行失败: {e}")))?;
    }

    let mut cmd = build_scp_process_command(connection).map_err(TransferJobError::message)?;
    let password_auth = is_password_auth(connection);
    cmd.args(build_scp_args(connection));
    cmd.arg(format!(
        "{}@{}:{}",
        connection.username,
        connection.host,
        shell_quote(remote_path)
    ));
    cmd.arg(local_path);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = timeout(Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| TransferJobError::timeout("下载超时"))?
        .map_err(|e| {
            TransferJobError::remote(map_command_spawn_error("下载失败", e, password_auth))
        })?;

    if output.status.success() {
        return Ok(());
    }

    Err(TransferJobError::remote(
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{RemoteTransferErrorCode, TransferJobError};

    #[test]
    fn classifies_cancelled_error() {
        let err = TransferJobError::Cancelled;
        assert!(err.is_cancelled());
        assert!(matches!(err, TransferJobError::Cancelled));
        assert_eq!(err.to_string(), "传输已取消");
    }

    #[test]
    fn classifies_io_error() {
        let err = TransferJobError::io("读取本地文件失败: permission denied");
        assert!(!err.is_cancelled());
        assert!(matches!(err, TransferJobError::Io(_)));
        assert_eq!(err.to_string(), "读取本地文件失败: permission denied");
    }

    #[test]
    fn classifies_timeout_error() {
        let err = TransferJobError::timeout("上传超时");
        assert!(!err.is_cancelled());
        assert!(matches!(err, TransferJobError::Timeout(_)));
        assert_eq!(err.to_string(), "上传超时");
    }

    #[test]
    fn classifies_remote_error() {
        let err = TransferJobError::remote("读取远端文件失败: permission denied");
        assert!(!err.is_cancelled());
        assert!(matches!(
            err,
            TransferJobError::Remote {
                code: RemoteTransferErrorCode::PermissionDenied,
                ..
            }
        ));
        assert_eq!(err.to_string(), "读取远端文件失败: permission denied");
    }

    #[test]
    fn classifies_remote_auth_failed_error_code() {
        let err = TransferJobError::remote("ssh authentication failed");
        assert!(matches!(
            err,
            TransferJobError::Remote {
                code: RemoteTransferErrorCode::AuthFailed,
                ..
            }
        ));
    }

    #[test]
    fn classifies_remote_path_not_found_error_code() {
        let err = TransferJobError::remote("No such file");
        assert!(matches!(
            err,
            TransferJobError::Remote {
                code: RemoteTransferErrorCode::PathNotFound,
                ..
            }
        ));
    }

    #[test]
    fn classifies_remote_network_disconnected_error_code() {
        let err = TransferJobError::remote("Connection reset by peer");
        assert!(matches!(
            err,
            TransferJobError::Remote {
                code: RemoteTransferErrorCode::NetworkDisconnected,
                ..
            }
        ));
    }

    #[test]
    fn classifies_remote_protocol_error_code_as_default() {
        let err = TransferJobError::remote("unknown sftp failure");
        assert!(matches!(
            err,
            TransferJobError::Remote {
                code: RemoteTransferErrorCode::Protocol,
                ..
            }
        ));
    }
}
