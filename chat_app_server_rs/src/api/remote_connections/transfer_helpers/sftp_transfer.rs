use ssh2::{OpenFlags, OpenType};
use std::io::{Read, Write};
use std::path::{Path as FsPath, PathBuf};
use tokio::time::Duration;
use walkdir::WalkDir;

use crate::models::remote_connection::RemoteConnection;

use super::super::{
    connect_ssh2_session_with_verification, join_remote_path, normalize_remote_path,
    remote_parent_path, SftpTransferManager,
};
use super::errors::TransferJobError;

fn check_transfer_not_cancelled(
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), TransferJobError> {
    if transfer_manager.is_cancel_requested(transfer_id) {
        return Err(TransferJobError::Cancelled);
    }
    Ok(())
}

pub(crate) fn estimate_local_total_bytes_typed(path: &FsPath) -> Result<u64, TransferJobError> {
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

pub(crate) fn run_sftp_transfer_job_typed(
    connection: &RemoteConnection,
    transfer_id: &str,
    direction: &str,
    local_path: &str,
    remote_path: &str,
    verification_code: Option<&str>,
    transfer_manager: &SftpTransferManager,
) -> Result<String, TransferJobError> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    let connected = connect_ssh2_session_with_verification(
        connection,
        Duration::from_secs(20),
        verification_code,
    )
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
