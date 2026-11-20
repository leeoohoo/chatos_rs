use super::*;

pub(super) fn local_source_root(
    source: &Path,
    subdirectory: Option<&str>,
) -> Result<PathBuf, String> {
    if !source.is_absolute() {
        return Err(install_error("本地 Plugin 来源必须是绝对目录"));
    }
    let metadata =
        fs::symlink_metadata(source).map_err(|_| install_error("本地 Plugin 目录不存在"))?;
    if !metadata.is_dir() || is_symlink_or_reparse(&metadata) {
        return Err(install_error("本地 Plugin 来源必须是普通目录"));
    }
    let root = source
        .canonicalize()
        .map_err(|_| install_error("本地 Plugin 目录无效"))?;
    let Some(subdirectory) = subdirectory.filter(|value| !value.is_empty()) else {
        return Ok(root);
    };
    if subdirectory.len() > 1_024 {
        return Err(install_error("Plugin 子目录无效"));
    }
    let relative = safe_relative_path(Path::new(subdirectory))?;
    let candidate = root.join(relative);
    let metadata =
        fs::symlink_metadata(&candidate).map_err(|_| install_error("Plugin 子目录不存在"))?;
    if !metadata.is_dir() || is_symlink_or_reparse(&metadata) {
        return Err(install_error("Plugin 子目录无效"));
    }
    let candidate = candidate
        .canonicalize()
        .map_err(|_| install_error("Plugin 子目录无效"))?;
    candidate
        .starts_with(&root)
        .then_some(candidate)
        .ok_or_else(|| install_error("Plugin 子目录越界"))
}

pub(super) fn snapshot_local_tree(
    source: &Path,
    destination: Option<&Path>,
) -> Result<String, String> {
    let mut entries = Vec::<(String, PathBuf, bool)>::new();
    collect_local_entries(source, source, &mut entries)?;
    if entries.is_empty() || entries.len() > MAXIMUM_ARCHIVE_ENTRIES {
        return Err(install_error("Plugin 目录为空或文件数量超过限制"));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    if entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(install_error("Plugin 目录包含大小写冲突路径"));
    }
    let mut digest = Sha256::new();
    let mut total_bytes = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1_024];
    for (identity, path, is_directory) in entries {
        if is_directory {
            digest.update(b"directory\0");
            digest.update(identity.as_bytes());
            digest.update([0]);
            if let Some(destination) = destination {
                let relative = path
                    .strip_prefix(source)
                    .map_err(|_| install_error("Plugin 路径越界"))?;
                ensure_owned_directory(&destination.join(relative))
                    .map_err(|_| install_error("无法复制 Plugin 目录"))?;
            }
            continue;
        }
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| install_error("无法读取 Plugin 文件"))?;
        if !metadata.is_file() || is_symlink_or_reparse(&metadata) {
            return Err(install_error("Plugin 目录包含链接或特殊文件"));
        }
        total_bytes = total_bytes
            .checked_add(metadata.len())
            .filter(|value| *value <= MAXIMUM_UNPACKED_BYTES)
            .ok_or_else(|| install_error("Plugin 目录体积超过限制"))?;
        digest.update(b"file\0");
        digest.update(identity.as_bytes());
        digest.update([0]);
        digest.update(metadata.len().to_le_bytes());
        let mut input = File::open(&path).map_err(|_| install_error("无法读取 Plugin 文件"))?;
        let mut output = if let Some(destination) = destination {
            let relative = path
                .strip_prefix(source)
                .map_err(|_| install_error("Plugin 路径越界"))?;
            let target = destination.join(relative);
            if let Some(parent) = target.parent() {
                ensure_owned_directory(parent)
                    .map_err(|_| install_error("无法复制 Plugin 目录"))?;
            }
            Some(create_private_file(&target).map_err(|_| install_error("无法复制 Plugin 文件"))?)
        } else {
            None
        };
        let mut read_bytes = 0_u64;
        loop {
            let count = input
                .read(&mut buffer)
                .map_err(|_| install_error("无法读取 Plugin 文件"))?;
            if count == 0 {
                break;
            }
            read_bytes += count as u64;
            digest.update(&buffer[..count]);
            if let Some(output) = output.as_mut() {
                output
                    .write_all(&buffer[..count])
                    .map_err(|_| install_error("无法复制 Plugin 文件"))?;
            }
        }
        if read_bytes != metadata.len() {
            return Err(install_error("Plugin 文件在检查期间发生变化"));
        }
        if let Some(output) = output.as_mut() {
            output
                .sync_all()
                .map_err(|_| install_error("无法保存 Plugin 文件"))?;
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn collect_local_entries(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<(String, PathBuf, bool)>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|_| install_error("无法读取 Plugin 目录"))? {
        let entry = entry.map_err(|_| install_error("无法读取 Plugin 目录"))?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| install_error("无法读取 Plugin 文件"))?;
        if is_symlink_or_reparse(&metadata) || (!metadata.is_dir() && !metadata.is_file()) {
            return Err(install_error("Plugin 目录包含链接或特殊文件"));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| install_error("Plugin 路径越界"))?;
        let identity = relative_path_identity(relative)?;
        entries.push((identity, path.clone(), metadata.is_dir()));
        if entries.len() > MAXIMUM_ARCHIVE_ENTRIES {
            return Err(install_error("Plugin 文件数量超过限制"));
        }
        if metadata.is_dir() {
            collect_local_entries(root, &path, entries)?;
        }
    }
    Ok(())
}

pub(super) fn safe_relative_path(path: &Path) -> Result<PathBuf, String> {
    let mut result = PathBuf::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            return Err(install_error("Plugin 相对路径无效"));
        };
        let value = value
            .to_str()
            .ok_or_else(|| install_error("Plugin 路径不是 UTF-8"))?;
        if !safe_windows_path_component(value) {
            return Err(install_error("Plugin 路径无效"));
        }
        result.push(value);
    }
    (!result.as_os_str().is_empty())
        .then_some(result)
        .ok_or_else(|| install_error("Plugin 相对路径无效"))
}

pub(super) fn relative_path_identity(path: &Path) -> Result<String, String> {
    if path.as_os_str().len() > 4_096 {
        return Err(install_error("Plugin 路径过长"));
    }
    let path = safe_relative_path(path)?;
    Ok(path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("/"))
}

pub(super) fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(windows)]
pub(super) fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
pub(super) fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
