use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_plugin_management_sdk::{
    parse_plugin_manifest, validate_plugin_manifest, verify_plugin_release_signature,
    PluginInstallSource, PluginManifest, PluginMcpServer, PluginReleaseVerificationContext,
};
use flate2::read::GzDecoder;
use reqwest::Response;
use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};
use tar::Archive;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::plugin_integrity::{remove_installation_integrity, write_installation_integrity};

const MAXIMUM_ARCHIVE_BYTES: u64 = 512 * 1_024 * 1_024;
const MAXIMUM_UNPACKED_BYTES: u64 = 512 * 1_024 * 1_024;
const MAXIMUM_ARCHIVE_ENTRIES: usize = 20_000;
const MAXIMUM_PACKAGE_JSON_BYTES: u64 = 1_024 * 1_024;

pub(crate) struct CommittedPluginPackage {
    pub(crate) plugin_root: PathBuf,
    pub(crate) installation_root: PathBuf,
    pub(crate) manifest: PluginManifest,
    pub(crate) artifact_sha256: String,
    backup_root: Option<PathBuf>,
}

impl CommittedPluginPackage {
    pub(crate) fn finish(mut self) -> Result<(), String> {
        if let Some(backup) = self.backup_root.take() {
            remove_confined_directory(&self.plugin_root, &backup)?;
        }
        Ok(())
    }

    pub(crate) fn rollback(mut self) {
        remove_installation_integrity(&self.plugin_root, &self.artifact_sha256);
        let _ = remove_confined_directory(&self.plugin_root, &self.installation_root);
        if let Some(backup) = self.backup_root.take() {
            if let Some(parent) = self.installation_root.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::rename(backup, &self.installation_root);
        }
    }
}

pub(crate) async fn install_marketplace_package(
    plugin_root: PathBuf,
    source: PluginInstallSource,
    response: Response,
) -> Result<CommittedPluginPackage, String> {
    validate_install_source(&source)?;
    fs::create_dir_all(&plugin_root).map_err(|_| install_error("无法创建 Plugin 安装目录"))?;
    let download_directory = plugin_root.join(".downloads");
    tokio::fs::create_dir_all(&download_directory)
        .await
        .map_err(|_| install_error("无法创建 Plugin 下载目录"))?;
    let archive_path = download_directory.join(format!("{}.tgz", Uuid::new_v4()));
    let download = download_verified_artifact(
        response,
        &archive_path,
        &source.release.artifact_sha256,
        &source.release.npm_package.integrity,
    )
    .await;
    if let Err(error) = download {
        let _ = tokio::fs::remove_file(&archive_path).await;
        return Err(error);
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = extract_and_commit(&plugin_root, &archive_path, &source);
        let _ = fs::remove_file(&archive_path);
        result
    })
    .await
    .map_err(|_| install_error("Plugin 安装任务异常退出"))?;
    result
}

async fn download_verified_artifact(
    mut response: Response,
    destination: &Path,
    expected_sha256: &str,
    npm_integrity: &str,
) -> Result<(), String> {
    if !response.status().is_success() {
        return Err(install_error("Plugin 安装包下载失败"));
    }
    if response
        .content_length()
        .is_some_and(|length| length == 0 || length > MAXIMUM_ARCHIVE_BYTES)
    {
        return Err(install_error("Plugin 安装包大小超过限制"));
    }
    let expected_sha512 = parse_npm_integrity(npm_integrity)?;
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .await
        .map_err(|_| install_error("无法保存 Plugin 安装包"))?;
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut written = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| install_error("Plugin 安装包下载中断"))?
    {
        written = written
            .checked_add(chunk.len() as u64)
            .filter(|value| *value <= MAXIMUM_ARCHIVE_BYTES)
            .ok_or_else(|| install_error("Plugin 安装包大小超过限制"))?;
        sha256.update(&chunk);
        sha512.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|_| install_error("无法保存 Plugin 安装包"))?;
    }
    if written == 0 {
        return Err(install_error("Plugin 安装包为空"));
    }
    file.flush()
        .await
        .map_err(|_| install_error("无法保存 Plugin 安装包"))?;
    file.sync_all()
        .await
        .map_err(|_| install_error("无法保存 Plugin 安装包"))?;
    drop(file);
    if format!("{:x}", sha256.finalize()) != expected_sha256
        || sha512.finalize().as_slice() != expected_sha512.as_slice()
    {
        return Err(install_error("Plugin 安装包完整性校验失败"));
    }
    Ok(())
}

fn extract_and_commit(
    plugin_root: &Path,
    archive_path: &Path,
    source: &PluginInstallSource,
) -> Result<CommittedPluginPackage, String> {
    let plugin_root = canonical_plugin_root(plugin_root)?;
    let staging_container = plugin_root
        .join(".staging")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&staging_container)
        .map_err(|_| install_error("无法创建 Plugin 临时安装目录"))?;
    let staged = (|| {
        extract_archive(archive_path, &staging_container)?;
        let package_root = staging_container.join("package");
        validate_extracted_package(&package_root, source)?;
        Ok(package_root)
    })();
    let package_root = match staged {
        Ok(value) => value,
        Err(error) => {
            let _ = remove_confined_directory(&plugin_root, &staging_container);
            return Err(error);
        }
    };

    let plugin_directory = plugin_root.join(sha256_text(&source.catalog.id));
    fs::create_dir_all(&plugin_directory).map_err(|_| install_error("无法创建 Plugin 版本目录"))?;
    let final_root = plugin_directory.join(sha256_text(&source.release.version));
    let backup_root = if final_root.exists() {
        let backup_directory = plugin_root.join(".backups");
        fs::create_dir_all(&backup_directory)
            .map_err(|_| install_error("无法创建 Plugin 回滚目录"))?;
        let backup = backup_directory.join(Uuid::new_v4().to_string());
        fs::rename(&final_root, &backup).map_err(|_| install_error("无法准备 Plugin 安装回滚"))?;
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(&package_root, &final_root) {
        if let Some(backup) = &backup_root {
            let _ = fs::rename(backup, &final_root);
        }
        let _ = remove_confined_directory(&plugin_root, &staging_container);
        return Err(install_error(&format!("无法提交 Plugin 安装：{error}")));
    }
    let _ = remove_confined_directory(&plugin_root, &staging_container);
    if let Err(error) =
        write_installation_integrity(&plugin_root, &final_root, &source.release.artifact_sha256)
    {
        let _ = remove_confined_directory(&plugin_root, &final_root);
        if let Some(backup) = &backup_root {
            let _ = fs::rename(backup, &final_root);
        }
        return Err(error);
    }
    Ok(CommittedPluginPackage {
        plugin_root,
        installation_root: final_root,
        manifest: source.release.normalized_manifest.clone(),
        artifact_sha256: source.release.artifact_sha256.clone(),
        backup_root,
    })
}

fn validate_install_source(source: &PluginInstallSource) -> Result<(), String> {
    validate_plugin_manifest(&source.release.normalized_manifest)
        .map_err(|_| install_error("Plugin Release 清单无效"))?;
    if !source.marketplace.enabled
        || source.marketplace.trust_level != "trusted"
        || !source.catalog.enabled
        || source.catalog.marketplace_id != source.marketplace.id
        || source.release.plugin_id != source.catalog.id
        || source.release.id != source.catalog.latest_release_id
        || source.release.version != source.release.normalized_manifest.version
        || source.release.manifest_schema_version
            != source.release.normalized_manifest.schema_version
        || source.release.revoked_at.is_some()
        || source.release.npm_package.version != source.release.version
        || !windows_compatible(&source.release.supported_platforms)
        || !windows_compatible(
            &source
                .release
                .normalized_manifest
                .dependencies
                .supported_platforms,
        )
    {
        return Err(install_error("Plugin Release 不可用于当前 Windows 客户端"));
    }
    let key = source
        .marketplace
        .trusted_signing_keys
        .iter()
        .find(|key| key.key_id == source.release.signature.key_id)
        .ok_or_else(|| install_error("Plugin Release 签名密钥不受信任"))?;
    verify_plugin_release_signature(
        PluginReleaseVerificationContext {
            plugin_id: &source.catalog.id,
            version: &source.release.version,
            marketplace_id: &source.marketplace.id,
            publisher_id: &source.catalog.publisher.id,
            artifact_sha256: &source.release.artifact_sha256,
        },
        &source.release.normalized_manifest,
        &source.release.signature,
        key,
    )
    .map_err(|_| install_error("Plugin Release 签名校验失败"))?;
    if !source
        .release
        .normalized_manifest
        .mcp_servers
        .iter()
        .any(|server| matches!(server, PluginMcpServer::Stdio { .. }))
    {
        return Err(install_error("Plugin 没有 Windows 可运行的 stdio MCP 组件"));
    }
    Ok(())
}

fn extract_archive(archive_path: &Path, staging_root: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|_| install_error("无法读取 Plugin 安装包"))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let mut observed = BTreeSet::new();
    let mut count = 0_usize;
    let mut total_bytes = 0_u64;
    let entries = archive
        .entries()
        .map_err(|_| install_error("Plugin 安装包格式无效"))?;
    for entry in entries {
        let mut entry = entry.map_err(|_| install_error("Plugin 安装包格式无效"))?;
        count += 1;
        if count > MAXIMUM_ARCHIVE_ENTRIES {
            return Err(install_error("Plugin 安装包文件数量超过限制"));
        }
        let path = entry
            .path()
            .map_err(|_| install_error("Plugin 安装包路径无效"))?
            .into_owned();
        let normalized = safe_archive_path(&path)?;
        if !observed.insert(archive_path_identity(&normalized)?) {
            return Err(install_error("Plugin 安装包包含重复路径"));
        }
        let kind = entry.header().entry_type();
        if kind.is_file() {
            total_bytes = total_bytes
                .checked_add(entry.size())
                .filter(|value| *value <= MAXIMUM_UNPACKED_BYTES)
                .ok_or_else(|| install_error("Plugin 解压体积超过限制"))?;
        } else if !kind.is_dir() {
            return Err(install_error("Plugin 安装包包含不允许的链接或特殊文件"));
        }
        if !entry
            .unpack_in(staging_root)
            .map_err(|_| install_error("Plugin 安装包解压失败"))?
        {
            return Err(install_error("Plugin 安装包包含越界路径"));
        }
    }
    if count == 0 || !staging_root.join("package").is_dir() {
        return Err(install_error("Plugin 安装包缺少 package 目录"));
    }
    Ok(())
}

fn validate_extracted_package(
    package_root: &Path,
    source: &PluginInstallSource,
) -> Result<(), String> {
    let package_root = package_root
        .canonicalize()
        .map_err(|_| install_error("Plugin 解压目录无效"))?;
    let manifest_path = package_root.join("chatos.plugin.json");
    let manifest_raw = read_bounded_text(&manifest_path, 2 * 1_024 * 1_024)?;
    let manifest =
        parse_plugin_manifest(&manifest_raw).map_err(|_| install_error("Plugin 安装包清单无效"))?;
    if manifest != source.release.normalized_manifest {
        return Err(install_error("Plugin 安装包清单与已签名 Release 不一致"));
    }
    let package_json = read_bounded_text(
        &package_root.join("package.json"),
        MAXIMUM_PACKAGE_JSON_BYTES,
    )?;
    let package_json: Value = serde_json::from_str(&package_json)
        .map_err(|_| install_error("Plugin package.json 无效"))?;
    if package_json.get("name").and_then(Value::as_str)
        != Some(source.release.npm_package.name.as_str())
        || package_json.get("version").and_then(Value::as_str)
            != Some(source.release.version.as_str())
        || !package_json.get("bin").is_some_and(|value| {
            value.is_string() || value.as_object().is_some_and(|items| !items.is_empty())
        })
    {
        return Err(install_error("Plugin package.json 身份或可执行入口无效"));
    }
    for server in &manifest.mcp_servers {
        let PluginMcpServer::Stdio { bin, .. } = server else {
            continue;
        };
        resolve_executable(&package_root, bin)?;
    }
    Ok(())
}

pub(crate) fn resolve_executable(installation_root: &Path, bin: &str) -> Result<PathBuf, String> {
    resolve_executable_for_arch(
        installation_root,
        bin,
        cfg!(windows).then_some(std::env::consts::ARCH),
    )
}

fn resolve_executable_for_arch(
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

fn read_bounded_text(path: &Path, maximum_bytes: u64) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| install_error("Plugin 文件缺失"))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
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

fn safe_archive_path(path: &Path) -> Result<PathBuf, String> {
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

fn archive_path_identity(path: &Path) -> Result<String, String> {
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

fn safe_windows_path_component(value: &str) -> bool {
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

fn parse_npm_integrity(value: &str) -> Result<Vec<u8>, String> {
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

fn canonical_plugin_root(path: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(path).map_err(|_| install_error("无法创建 Plugin 安装目录"))?;
    let metadata = fs::symlink_metadata(path).map_err(|_| install_error("Plugin 安装目录无效"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(install_error("Plugin 安装目录无效"));
    }
    path.canonicalize()
        .map_err(|_| install_error("Plugin 安装目录无效"))
}

fn remove_confined_directory(plugin_root: &Path, requested: &Path) -> Result<(), String> {
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

fn windows_compatible(platforms: &[String]) -> bool {
    platforms.is_empty()
        || platforms.iter().any(|platform| {
            let platform = platform.trim().to_ascii_lowercase();
            platform == "windows" || platform.starts_with("windows-")
        })
}

fn valid_executable_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}

fn sha256_text(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn install_error(message: &str) -> String {
    message.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_paths_are_confined_to_package() {
        assert_eq!(
            safe_archive_path(Path::new("package/bin/tool.exe")).unwrap(),
            PathBuf::from("package/bin/tool.exe")
        );
        assert!(safe_archive_path(Path::new("../escape")).is_err());
        assert!(safe_archive_path(Path::new("other/file")).is_err());
        assert!(safe_archive_path(Path::new("/package/file")).is_err());
        assert!(safe_archive_path(Path::new("package/CON/file")).is_err());
        assert!(safe_archive_path(Path::new("package/file:stream")).is_err());
        assert!(safe_archive_path(Path::new("package/file. ")).is_err());
    }

    #[test]
    fn npm_integrity_requires_sha512() {
        let expected = vec![7_u8; 64];
        let encoded = format!("sha512-{}", STANDARD.encode(&expected));
        assert_eq!(parse_npm_integrity(&encoded).unwrap(), expected);
        assert!(parse_npm_integrity("sha256-bad").is_err());
    }

    #[test]
    fn executable_resolution_prefers_the_current_windows_architecture() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let bin = root.join("bin");
        let x64 = bin.join("windows-x64");
        let arm64 = bin.join("windows-arm64");
        std::fs::create_dir_all(&x64).unwrap();
        std::fs::create_dir_all(&arm64).unwrap();
        std::fs::write(bin.join("open-computer-use"), "launcher").unwrap();
        std::fs::write(x64.join("open-computer-use.exe"), "x64").unwrap();
        std::fs::write(arm64.join("open-computer-use.exe"), "arm64").unwrap();

        assert_eq!(
            resolve_executable_for_arch(&root, "open-computer-use", Some("x86_64")).unwrap(),
            x64.join("open-computer-use.exe").canonicalize().unwrap()
        );
        assert_eq!(
            resolve_executable_for_arch(&root, "open-computer-use", Some("aarch64")).unwrap(),
            arm64.join("open-computer-use.exe").canonicalize().unwrap()
        );
        assert_eq!(
            resolve_executable_for_arch(&root, "open-computer-use", None).unwrap(),
            bin.join("open-computer-use").canonicalize().unwrap()
        );
    }
}
