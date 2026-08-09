// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io::{Read, Write};
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use ssh2::{OpenFlags, OpenType, Sftp};

use super::transfer::TransferProgress;
use super::SftpBody;

pub(super) fn list_entries(sftp: &Sftp, path: &str) -> Result<Value, String> {
    let normalized = chatos_remote_runtime::normalize_remote_path(path);
    let mut entries = sftp
        .readdir(Path::new(normalized.as_str()))
        .map_err(|error| format!("read remote directory failed: {error}"))?
        .into_iter()
        .filter_map(|(path, stat)| {
            let name = path.file_name()?.to_string_lossy().to_string();
            (!matches!(name.as_str(), "." | "..")).then(|| {
                json!({
                    "name": name,
                    "path": remote_path_to_string(path.as_path()),
                    "is_dir": stat.is_dir(),
                    "is_symlink": stat.file_type().is_symlink(),
                    "size": stat.size,
                    "modified_at": stat.mtime.and_then(|value| {
                        chrono::DateTime::<chrono::Utc>::from_timestamp(value as i64, 0)
                            .map(|timestamp| timestamp.to_rfc3339())
                    }),
                })
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        let left_dir = left.get("is_dir").and_then(Value::as_bool).unwrap_or(false);
        let right_dir = right
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        right_dir.cmp(&left_dir).then_with(|| {
            left.get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_lowercase()
                .cmp(
                    &right
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_lowercase(),
                )
        })
    });
    Ok(json!({
        "path": normalized,
        "parent": chatos_remote_runtime::remote_parent_path(path),
        "entries": entries,
    }))
}

pub(super) fn create_directory(sftp: &Sftp, parent: &str, name: &str) -> Result<Value, String> {
    validate_entry_name(name)?;
    let target = chatos_remote_runtime::join_remote_path(parent, name);
    ensure_remote_dir(sftp, target.as_str())?;
    Ok(json!({ "success": true, "path": target }))
}

pub(super) fn rename_entry(sftp: &Sftp, from_path: &str, to_path: &str) -> Result<Value, String> {
    reject_remote_root(from_path, "rename")?;
    sftp.rename(Path::new(from_path), Path::new(to_path), None)
        .map_err(|error| format!("rename remote entry failed: {error}"))?;
    Ok(json!({ "success": true }))
}

pub(super) fn delete_entry(sftp: &Sftp, path: &str, recursive: bool) -> Result<Value, String> {
    delete_remote(sftp, path, recursive)?;
    Ok(json!({ "success": true }))
}

pub(super) fn read_remote_file(sftp: &Sftp, path: &str, max_bytes: usize) -> Result<Value, String> {
    let stat = sftp
        .lstat(Path::new(path))
        .map_err(|error| format!("stat remote file failed: {error}"))?;
    if !stat.is_file() {
        return Err("remote path must be a regular file".to_string());
    }
    let mut file = sftp
        .open(Path::new(path))
        .map_err(|error| format!("open remote file failed: {error}"))?;
    let mut content = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut content)
        .map_err(|error| format!("read remote file failed: {error}"))?;
    let truncated =
        content.len() > max_bytes || stat.size.is_some_and(|size| size > max_bytes as u64);
    content.truncate(max_bytes);
    Ok(json!({
        "content_base64": BASE64_STANDARD.encode(content),
        "source_size": stat.size,
        "truncated": truncated,
    }))
}

pub(super) fn write_remote_file(sftp: &Sftp, body: &SftpBody) -> Result<Value, String> {
    let path = super::required(&body.remote_path, "remote_path")?;
    let content = BASE64_STANDARD
        .decode(super::required(&body.content_base64, "content_base64")?)
        .map_err(|error| format!("invalid base64 content: {error}"))?;
    if body.create_parent_dirs.unwrap_or(false) {
        if let Some(parent) = chatos_remote_runtime::remote_parent_path(path) {
            ensure_remote_dir(sftp, parent.as_str())?;
        }
    }
    if !body.overwrite.unwrap_or(false) && sftp.lstat(Path::new(path)).is_ok() {
        return Err(format!("remote file already exists: {path}"));
    }
    let mut file = sftp
        .open_mode(
            Path::new(path),
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .map_err(|error| format!("open remote file for write failed: {error}"))?;
    file.write_all(content.as_slice())
        .map_err(|error| format!("write remote file failed: {error}"))?;
    file.flush()
        .map_err(|error| format!("flush remote file failed: {error}"))?;
    Ok(json!({ "bytes_written": content.len() }))
}

pub(super) fn upload_path(
    sftp: &Sftp,
    local: &Path,
    remote: &str,
    progress: Option<&TransferProgress>,
) -> Result<String, String> {
    let total = estimate_local_total_bytes(local, progress)?;
    if let Some(progress) = progress {
        progress.set_total(total, local.display().to_string());
    }
    copy_local_to_remote(sftp, local, remote, progress)?;
    Ok(if local.is_dir() {
        "目录上传完成".to_string()
    } else {
        "文件上传完成".to_string()
    })
}

pub(super) fn download_path(
    sftp: &Sftp,
    remote: &str,
    local: &Path,
    progress: Option<&TransferProgress>,
) -> Result<String, String> {
    let stat = sftp
        .lstat(Path::new(remote))
        .map_err(|error| format!("stat remote entry failed: {error}"))?;
    reject_remote_symlink(&stat)?;
    let total = estimate_remote_total_bytes(sftp, remote, &stat, progress)?;
    if let Some(progress) = progress {
        progress.set_total(total, remote.to_string());
    }
    copy_remote_to_local(sftp, remote, &stat, local, progress)?;
    Ok(if stat.is_dir() {
        "目录下载完成".to_string()
    } else {
        "文件下载完成".to_string()
    })
}

fn ensure_remote_dir(sftp: &Sftp, path: &str) -> Result<(), String> {
    let normalized = chatos_remote_runtime::normalize_remote_path(path);
    if matches!(normalized.as_str(), "." | "/") {
        return Ok(());
    }
    let absolute = normalized.starts_with('/');
    let mut current = if absolute {
        "/".to_string()
    } else {
        String::new()
    };
    for part in normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
    {
        current = if current.is_empty() {
            part.to_string()
        } else if current == "/" {
            format!("/{part}")
        } else {
            format!("{current}/{part}")
        };
        let target = Path::new(current.as_str());
        match sftp.stat(target) {
            Ok(stat) if stat.is_dir() => continue,
            Ok(_) => return Err(format!("remote path is not a directory: {current}")),
            Err(_) => {}
        }
        if let Err(error) = sftp.mkdir(target, 0o755) {
            match sftp.stat(target) {
                Ok(stat) if stat.is_dir() => {}
                _ => {
                    return Err(format!(
                        "create remote directory failed ({current}): {error}"
                    ))
                }
            }
        }
    }
    Ok(())
}

fn delete_remote(sftp: &Sftp, path: &str, recursive: bool) -> Result<(), String> {
    reject_remote_root(path, "delete")?;
    let target = Path::new(path);
    let stat = sftp
        .lstat(target)
        .map_err(|error| format!("stat remote entry failed: {error}"))?;
    if stat.is_dir() {
        if recursive {
            for (child, child_stat) in sftp
                .readdir(target)
                .map_err(|error| format!("read remote directory failed: {error}"))?
            {
                let name = remote_entry_name(child.as_path())?;
                if matches!(name.as_str(), "." | "..") {
                    continue;
                }
                if child_stat.is_dir() && !child_stat.file_type().is_symlink() {
                    delete_remote(sftp, remote_path_to_string(child.as_path()).as_str(), true)?;
                } else {
                    sftp.unlink(child.as_path())
                        .map_err(|error| format!("remove remote file failed: {error}"))?;
                }
            }
        }
        sftp.rmdir(target)
            .map_err(|error| format!("remove remote directory failed: {error}"))
    } else {
        sftp.unlink(target)
            .map_err(|error| format!("remove remote file failed: {error}"))
    }
}

fn estimate_local_total_bytes(
    path: &Path,
    progress: Option<&TransferProgress>,
) -> Result<u64, String> {
    check_progress(progress)?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("read local path metadata failed: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "local transfer path cannot be a symbolic link: {}",
            path.display()
        ));
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if metadata.is_dir() {
        let mut total = 0u64;
        for entry in std::fs::read_dir(path)
            .map_err(|error| format!("read local directory failed: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("read local directory entry failed: {error}"))?;
            total = total.saturating_add(estimate_local_total_bytes(
                entry.path().as_path(),
                progress,
            )?);
        }
        return Ok(total);
    }
    Err("local transfer path must be a regular file or directory".to_string())
}

fn estimate_remote_total_bytes(
    sftp: &Sftp,
    remote: &str,
    stat: &ssh2::FileStat,
    progress: Option<&TransferProgress>,
) -> Result<u64, String> {
    check_progress(progress)?;
    reject_remote_symlink(stat)?;
    if stat.is_file() {
        return Ok(stat.size.unwrap_or(0));
    }
    if stat.is_dir() {
        let mut total = 0u64;
        for (child, child_stat) in sftp
            .readdir(Path::new(remote))
            .map_err(|error| format!("read remote directory failed: {error}"))?
        {
            let name = remote_entry_name(child.as_path())?;
            if matches!(name.as_str(), "." | "..") {
                continue;
            }
            total = total.saturating_add(estimate_remote_total_bytes(
                sftp,
                remote_path_to_string(child.as_path()).as_str(),
                &child_stat,
                progress,
            )?);
        }
        return Ok(total);
    }
    Err("remote transfer path must be a regular file or directory".to_string())
}

fn copy_local_to_remote(
    sftp: &Sftp,
    local: &Path,
    remote: &str,
    progress: Option<&TransferProgress>,
) -> Result<(), String> {
    check_progress(progress)?;
    let metadata = std::fs::symlink_metadata(local)
        .map_err(|error| format!("read local path metadata failed: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "local transfer path cannot be a symbolic link: {}",
            local.display()
        ));
    }
    if metadata.is_dir() {
        ensure_remote_dir(sftp, remote)?;
        for entry in std::fs::read_dir(local)
            .map_err(|error| format!("read local directory failed: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("read local directory entry failed: {error}"))?;
            let child_remote = chatos_remote_runtime::join_remote_path(
                remote,
                entry.file_name().to_string_lossy().as_ref(),
            );
            copy_local_to_remote(
                sftp,
                entry.path().as_path(),
                child_remote.as_str(),
                progress,
            )?;
        }
        return Ok(());
    }
    if !metadata.is_file() {
        return Err("local transfer path must be a regular file or directory".to_string());
    }
    if let Some(parent) = chatos_remote_runtime::remote_parent_path(remote) {
        ensure_remote_dir(sftp, parent.as_str())?;
    }
    let mut source =
        std::fs::File::open(local).map_err(|error| format!("open local file failed: {error}"))?;
    let mut target = sftp
        .open_mode(
            Path::new(remote),
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .map_err(|error| format!("open remote file failed: {error}"))?;
    copy_stream(
        &mut source,
        &mut target,
        local.display().to_string(),
        progress,
    )
}

fn copy_remote_to_local(
    sftp: &Sftp,
    remote: &str,
    stat: &ssh2::FileStat,
    local: &Path,
    progress: Option<&TransferProgress>,
) -> Result<(), String> {
    check_progress(progress)?;
    reject_remote_symlink(stat)?;
    reject_local_symlink(local)?;
    if stat.is_dir() {
        if local.exists() && !local.is_dir() {
            return Err("local download target exists and is not a directory".to_string());
        }
        std::fs::create_dir_all(local)
            .map_err(|error| format!("create local directory failed: {error}"))?;
        for (child, child_stat) in sftp
            .readdir(Path::new(remote))
            .map_err(|error| format!("read remote directory failed: {error}"))?
        {
            let name = remote_entry_name(child.as_path())?;
            if matches!(name.as_str(), "." | "..") {
                continue;
            }
            copy_remote_to_local(
                sftp,
                remote_path_to_string(child.as_path()).as_str(),
                &child_stat,
                local.join(name).as_path(),
                progress,
            )?;
        }
        return Ok(());
    }
    if !stat.is_file() {
        return Err("remote transfer path must be a regular file or directory".to_string());
    }
    if local.exists() && !local.is_file() {
        return Err("local download target exists and is not a regular file".to_string());
    }
    if let Some(parent) = local.parent() {
        reject_local_symlink(parent)?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create local parent failed: {error}"))?;
    }
    let mut source = sftp
        .open(Path::new(remote))
        .map_err(|error| format!("open remote file failed: {error}"))?;
    let mut target = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(local)
        .map_err(|error| format!("create local file failed: {error}"))?;
    copy_stream(&mut source, &mut target, remote.to_string(), progress)
}

fn copy_stream(
    source: &mut impl Read,
    target: &mut impl Write,
    path: String,
    progress: Option<&TransferProgress>,
) -> Result<(), String> {
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_progress(progress)?;
        let size = source
            .read(&mut buffer)
            .map_err(|error| format!("read transfer source failed: {error}"))?;
        if size == 0 {
            break;
        }
        target
            .write_all(&buffer[..size])
            .map_err(|error| format!("write transfer target failed: {error}"))?;
        if let Some(progress) = progress {
            progress.add(size as u64, path.clone());
        }
    }
    target
        .flush()
        .map_err(|error| format!("flush transfer target failed: {error}"))
}

fn reject_remote_root(path: &str, operation: &str) -> Result<(), String> {
    let normalized = chatos_remote_runtime::normalize_remote_path(path);
    if matches!(normalized.as_str(), "." | "/") {
        Err(format!("refusing to {operation} the remote root directory"))
    } else {
        Ok(())
    }
}

fn reject_remote_symlink(stat: &ssh2::FileStat) -> Result<(), String> {
    if stat.file_type().is_symlink() {
        Err("symbolic links are not supported for recursive SFTP transfers".to_string())
    } else {
        Ok(())
    }
}

fn reject_local_symlink(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "local transfer path cannot be a symbolic link: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("read local path metadata failed: {error}")),
    }
}

fn validate_entry_name(name: &str) -> Result<(), String> {
    if name.is_empty() || matches!(name, "." | "..") || name.contains(['/', '\\']) {
        Err("invalid remote directory name".to_string())
    } else {
        Ok(())
    }
}

fn remote_entry_name(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "remote directory contains an invalid entry name".to_string())
}

fn remote_path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn check_progress(progress: Option<&TransferProgress>) -> Result<(), String> {
    match progress {
        Some(progress) => progress.check(),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_destructive_remote_root_operations() {
        assert!(reject_remote_root("/", "delete").is_err());
        assert!(reject_remote_root(" . ", "rename").is_err());
        assert!(reject_remote_root("/srv/app", "delete").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn local_size_scan_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("chatos-sftp-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.as_path()).unwrap();
        let source = root.join("source.txt");
        let link = root.join("link.txt");
        std::fs::write(source.as_path(), b"data").unwrap();
        symlink(source.as_path(), link.as_path()).unwrap();

        let error = estimate_local_total_bytes(link.as_path(), None).unwrap_err();
        assert!(error.contains("symbolic link"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
