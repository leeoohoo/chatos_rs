use std::path::Path as FsPath;
use std::process::Stdio;
use tokio::time::{timeout, Duration};

use crate::models::remote_connection::RemoteConnection;

use super::super::{
    build_scp_args, build_scp_process_command, connect_ssh2_session, is_password_auth,
    map_command_spawn_error, shell_quote, should_use_native_ssh,
};
use super::errors::TransferJobError;
use ssh2::{OpenFlags, OpenType};

pub(crate) async fn run_scp_upload_typed(
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

pub(crate) async fn run_scp_download_typed(
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
