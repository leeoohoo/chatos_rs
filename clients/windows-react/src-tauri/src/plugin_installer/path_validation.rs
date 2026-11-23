use super::*;

pub(super) fn resolve_executable_for_arch(
    installation_root: &Path,
    bin: &str,
    windows_arch: Option<&str>,
) -> Result<PathBuf, String> {
    if !valid_executable_name(bin) {
        return Err(install_error("Plugin 可执行文件名无效"));
    }
    let bin_root = installation_root.join("bin");
    let candidates = executable_candidates(&bin_root, bin, windows_arch);
    for candidate in candidates {
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(install_error("无法读取 Plugin 可执行文件")),
        };
        if metadata.is_file() && !metadata.file_type().is_symlink() {
            let canonical = candidate
                .canonicalize()
                .map_err(|_| install_error("Plugin 可执行文件无效"))?;
            if canonical.starts_with(installation_root) {
                return Ok(canonical);
            }
        }
    }
    Err(install_error("Plugin 可执行文件不存在"))
}

fn executable_candidates(bin_root: &Path, bin: &str, windows_arch: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(3);
    if let Some(runtime_directory) = windows_arch.and_then(|architecture| match architecture {
        "x86_64" => Some("windows-x64"),
        "aarch64" => Some("windows-arm64"),
        _ => None,
    }) {
        candidates.push(bin_root.join(runtime_directory).join(format!("{bin}.exe")));
    }
    if windows_arch.is_some() {
        candidates.push(bin_root.join(format!("{bin}.exe")));
        candidates.push(bin_root.join(bin));
    } else {
        candidates.push(bin_root.join(bin));
        candidates.push(bin_root.join(format!("{bin}.exe")));
    }
    candidates
}

pub(super) fn read_bounded_text(path: &Path, maximum_bytes: u64) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| install_error("Plugin 文件缺失"))?;
    if !metadata.is_file()
        || is_symlink_or_reparse(&metadata)
        || metadata.len() == 0
        || metadata.len() > maximum_bytes
    {
        return Err(install_error("Plugin 文件大小或类型无效"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|file| file.take(maximum_bytes + 1).read_to_end(&mut bytes))
        .map_err(|_| install_error("无法读取 Plugin 文件"))?;
    if bytes.len() as u64 > maximum_bytes {
        return Err(install_error("Plugin 文件大小超过限制"));
    }
    String::from_utf8(bytes).map_err(|_| install_error("Plugin 文件不是 UTF-8"))
}

pub(super) fn safe_archive_path(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().len() > 4_096 {
        return Err(install_error("Plugin 安装包路径过长"));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(install_error("Plugin 安装包包含越界路径"));
        };
        let value = component
            .to_str()
            .ok_or_else(|| install_error("Plugin 安装包路径不是 UTF-8"))?;
        if !safe_windows_path_component(value) {
            return Err(install_error("Plugin 安装包路径无效"));
        }
        normalized.push(value);
    }
    if normalized
        .components()
        .next()
        .and_then(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        != Some("package")
    {
        return Err(install_error("Plugin 安装包路径必须位于 package 目录"));
    }
    Ok(normalized)
}

pub(super) fn archive_path_identity(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(install_error("Plugin 安装包路径无效"));
        };
        parts.push(
            component
                .to_str()
                .ok_or_else(|| install_error("Plugin 安装包路径不是 UTF-8"))?
                .to_ascii_lowercase(),
        );
    }
    Ok(parts.join("/"))
}

pub(super) fn safe_windows_path_component(value: &str) -> bool {
    if value.is_empty()
        || value.ends_with(['.', ' '])
        || value.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
    {
        return false;
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

pub(super) fn parse_npm_integrity(value: &str) -> Result<Vec<u8>, String> {
    let encoded = value
        .trim()
        .strip_prefix("sha512-")
        .ok_or_else(|| install_error("Plugin npm integrity 格式无效"))?;
    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| install_error("Plugin npm integrity 格式无效"))?;
    if decoded.len() != 64 {
        return Err(install_error("Plugin npm integrity 长度无效"));
    }
    Ok(decoded)
}

pub(super) fn canonical_plugin_root(path: &Path) -> Result<PathBuf, String> {
    ensure_owned_directory(path).map_err(|_| install_error("无法创建 Plugin 安装目录"))?;
    let metadata = fs::symlink_metadata(path).map_err(|_| install_error("Plugin 安装目录无效"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(install_error("Plugin 安装目录无效"));
    }
    path.canonicalize()
        .map_err(|_| install_error("Plugin 安装目录无效"))
}

pub(super) fn remove_confined_directory(
    plugin_root: &Path,
    requested: &Path,
) -> Result<(), String> {
    if !requested.exists() {
        return Ok(());
    }
    let root = plugin_root
        .canonicalize()
        .map_err(|_| install_error("Plugin 安装目录无效"))?;
    let requested = requested
        .canonicalize()
        .map_err(|_| install_error("Plugin 安装目录无效"))?;
    if requested == root || !requested.starts_with(&root) || !requested.is_dir() {
        return Err(install_error("拒绝删除越界的 Plugin 目录"));
    }
    fs::remove_dir_all(requested).map_err(|_| install_error("Plugin 临时目录清理失败"))
}

pub(super) fn valid_executable_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}

pub(super) fn sha256_text(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub(super) fn install_error(message: &str) -> String {
    message.to_owned()
}
