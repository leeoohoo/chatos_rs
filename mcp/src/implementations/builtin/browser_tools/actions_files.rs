// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};
use uuid::Uuid;

use super::actions_shared::{
    copy_response_fields, fail_json, finalize_browser_action_response, is_success, normalize_ref,
    run_basic_browser_action, run_browser_command,
};
use super::BoundContext;

pub(super) const MAX_BROWSER_UPLOAD_FILES: usize = 10;
pub(super) const MAX_BROWSER_UPLOAD_FILE_BYTES: u64 = 50 * 1024 * 1024;
pub(super) const MAX_BROWSER_UPLOAD_TOTAL_BYTES: u64 = 100 * 1024 * 1024;
pub(super) const MAX_BROWSER_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024;
const MAX_BROWSER_FILE_PATH_CHARS: usize = 4_096;
const DOWNLOAD_STAGING_PREFIX: &str = ".chatos-browser-download-";

#[derive(Debug)]
struct UploadFile {
    absolute: PathBuf,
    relative: String,
    bytes: u64,
}

#[derive(Debug)]
struct DownloadDestination {
    target: PathBuf,
    staging: PathBuf,
    relative: String,
}

pub(super) async fn browser_upload_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    reference: String,
    paths: Vec<String>,
) -> Result<Value, String> {
    let files = resolve_upload_files(ctx.workspace_dir.as_path(), paths.as_slice())?;
    let total_bytes = files.iter().map(|file| file.bytes).sum::<u64>();
    let file_rows = files
        .iter()
        .map(|file| json!({"path": file.relative, "bytes": file.bytes}))
        .collect::<Vec<_>>();
    let mut args = Vec::with_capacity(files.len() + 1);
    let reference = normalize_ref(reference);
    args.push(reference.clone());
    args.extend(
        files
            .iter()
            .map(|file| file.absolute.to_string_lossy().to_string()),
    );
    let session = super::super::context::conversation_key(conversation_id);

    run_basic_browser_action(
        &ctx,
        session.as_str(),
        "upload",
        args,
        ctx.command_timeout_seconds.max(120),
        format!("Failed to upload files into {reference}"),
        json!({
            "success": true,
            "element": reference,
            "file_count": file_rows.len(),
            "total_bytes": total_bytes,
            "files": file_rows,
        }),
        "Uploaded workspace file(s) into the page file input.",
        Some("The page snapshot was refreshed after the upload."),
    )
    .await
}

pub(super) async fn browser_download_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    reference: String,
    path: String,
) -> Result<Value, String> {
    let destination = prepare_download_destination(ctx.workspace_dir.as_path(), path.as_str())?;
    let reference = normalize_ref(reference);
    let session = super::super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "download",
        vec![
            reference.clone(),
            destination.staging.to_string_lossy().to_string(),
        ],
        ctx.command_timeout_seconds.max(120),
    )
    .await?;
    if !is_success(&result) {
        remove_regular_file_or_symlink(destination.staging.as_path());
        return Ok(fail_json(&result, "Browser download failed"));
    }

    let bytes = match publish_download(&destination) {
        Ok(bytes) => bytes,
        Err(error) => {
            remove_regular_file_or_symlink(destination.staging.as_path());
            remove_regular_file_or_symlink(destination.target.as_path());
            let mut response = json!({
                "_summary_text": format!("Browser download failed: {error}."),
                "success": false,
                "error": error,
                "path": destination.relative,
            });
            copy_response_fields(&mut response, &result, &["browser_session"]);
            return Ok(response);
        }
    };

    Ok(finalize_browser_action_response(
        &ctx,
        session.as_str(),
        json!({
            "success": true,
            "element": reference,
            "path": destination.relative,
            "bytes": bytes,
            "overwrote_existing": false,
        }),
        "Downloaded a browser file into the workspace without overwriting an existing path.",
        Some("The downloaded file is now available through workspace file tools."),
    )
    .await)
}

fn resolve_upload_files(
    workspace_root: &Path,
    paths: &[String],
) -> Result<Vec<UploadFile>, String> {
    if paths.is_empty() || paths.len() > MAX_BROWSER_UPLOAD_FILES {
        return Err(format!(
            "paths must contain between 1 and {MAX_BROWSER_UPLOAD_FILES} workspace-relative files"
        ));
    }

    let mut files = Vec::with_capacity(paths.len());
    let mut total_bytes = 0_u64;
    for requested in paths {
        let (relative_path, relative) = normalize_workspace_relative_path(requested.as_str())?;
        let candidate = workspace_root.join(relative_path);
        let link_metadata = fs::symlink_metadata(candidate.as_path())
            .map_err(|error| format!("read upload file {relative} failed: {error}"))?;
        if link_metadata.file_type().is_symlink() || !link_metadata.file_type().is_file() {
            return Err(format!(
                "upload path must be a regular non-symlink file: {relative}"
            ));
        }
        let canonical = candidate
            .canonicalize()
            .map_err(|error| format!("canonicalize upload file {relative} failed: {error}"))?;
        if !canonical.starts_with(workspace_root) {
            return Err(format!("upload path escapes the workspace: {relative}"));
        }
        let bytes = link_metadata.len();
        if bytes > MAX_BROWSER_UPLOAD_FILE_BYTES {
            return Err(format!(
                "upload file exceeds {MAX_BROWSER_UPLOAD_FILE_BYTES} bytes: {relative}"
            ));
        }
        total_bytes = total_bytes
            .checked_add(bytes)
            .ok_or_else(|| "upload file sizes overflowed".to_string())?;
        if total_bytes > MAX_BROWSER_UPLOAD_TOTAL_BYTES {
            return Err(format!(
                "upload files exceed {MAX_BROWSER_UPLOAD_TOTAL_BYTES} total bytes"
            ));
        }
        files.push(UploadFile {
            absolute: canonical,
            relative,
            bytes,
        });
    }
    Ok(files)
}

fn prepare_download_destination(
    workspace_root: &Path,
    requested: &str,
) -> Result<DownloadDestination, String> {
    let (relative_path, relative) = normalize_workspace_relative_path(requested)?;
    let file_name = relative_path
        .file_name()
        .ok_or_else(|| "download path must include a file name".to_string())?;
    if file_name
        .to_string_lossy()
        .starts_with(DOWNLOAD_STAGING_PREFIX)
    {
        return Err("download file name uses a reserved ChatOS prefix".to_string());
    }
    let parent_relative = relative_path.parent().unwrap_or_else(|| Path::new(""));
    let parent_candidate = workspace_root.join(parent_relative);
    let parent = parent_candidate
        .canonicalize()
        .map_err(|error| format!("canonicalize download parent failed: {error}"))?;
    if !parent.starts_with(workspace_root) || !parent.is_dir() {
        return Err("download parent must be an existing workspace directory".to_string());
    }
    let target = parent.join(file_name);
    if fs::symlink_metadata(target.as_path()).is_ok() {
        return Err(format!("download target already exists: {relative}"));
    }
    let staging = parent.join(format!(
        "{DOWNLOAD_STAGING_PREFIX}{}.part",
        Uuid::new_v4().simple()
    ));
    if fs::symlink_metadata(staging.as_path()).is_ok() {
        return Err("download staging path unexpectedly exists".to_string());
    }
    Ok(DownloadDestination {
        target,
        staging,
        relative,
    })
}

fn publish_download(destination: &DownloadDestination) -> Result<u64, String> {
    let metadata = fs::symlink_metadata(destination.staging.as_path())
        .map_err(|error| format!("browser did not create the expected download: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("browser download output is not a regular non-symlink file".to_string());
    }
    if metadata.len() > MAX_BROWSER_DOWNLOAD_BYTES {
        return Err(format!(
            "browser download exceeds {MAX_BROWSER_DOWNLOAD_BYTES} bytes"
        ));
    }

    let source = fs::File::open(destination.staging.as_path())
        .map_err(|error| format!("open browser download failed: {error}"))?;
    let mut limited_source = source.take(MAX_BROWSER_DOWNLOAD_BYTES + 1);
    let mut target = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination.target.as_path())
        .map_err(|error| format!("create download target without overwrite failed: {error}"))?;
    let copied = io::copy(&mut limited_source, &mut target)
        .map_err(|error| format!("copy browser download failed: {error}"))?;
    if copied > MAX_BROWSER_DOWNLOAD_BYTES {
        return Err(format!(
            "browser download exceeds {MAX_BROWSER_DOWNLOAD_BYTES} bytes"
        ));
    }
    target
        .flush()
        .map_err(|error| format!("flush browser download failed: {error}"))?;
    target
        .sync_all()
        .map_err(|error| format!("sync browser download failed: {error}"))?;
    if copied != metadata.len() {
        return Err("browser download size changed while publishing".to_string());
    }
    fs::remove_file(destination.staging.as_path())
        .map_err(|error| format!("remove browser download staging file failed: {error}"))?;

    let target_metadata = fs::symlink_metadata(destination.target.as_path())
        .map_err(|error| format!("verify browser download target failed: {error}"))?;
    if target_metadata.file_type().is_symlink()
        || !target_metadata.file_type().is_file()
        || target_metadata.len() != copied
    {
        return Err("browser download target verification failed".to_string());
    }
    Ok(copied)
}

pub(super) fn normalize_workspace_relative_path(
    requested: &str,
) -> Result<(PathBuf, String), String> {
    if requested.is_empty()
        || requested.chars().count() > MAX_BROWSER_FILE_PATH_CHARS
        || requested.chars().any(char::is_control)
    {
        return Err(
            "browser file path is empty, too long, or contains control characters".to_string(),
        );
    }
    let mut normalized = PathBuf::new();
    for component in Path::new(requested).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(
                    "browser file path must be workspace-relative without parent traversal"
                        .to_string(),
                )
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err("browser file path must name a workspace file".to_string());
    }
    let relative = normalized.to_string_lossy().replace('\\', "/");
    Ok((normalized, relative))
}

pub(super) fn remove_regular_file_or_symlink(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_workspace(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "chatos_browser_files_{label}_{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(root.as_path()).expect("create test workspace");
        root.canonicalize().expect("canonical test workspace")
    }

    #[test]
    fn browser_file_paths_reject_absolute_and_parent_traversal() {
        assert!(normalize_workspace_relative_path("../secret.txt").is_err());
        assert!(normalize_workspace_relative_path("/tmp/secret.txt").is_err());
        assert!(normalize_workspace_relative_path("safe/file.txt").is_ok());
    }

    #[test]
    fn upload_files_are_regular_bounded_workspace_files() {
        let root = test_workspace("upload");
        fs::write(root.join("one.txt"), b"one").expect("write upload file");
        let files = resolve_upload_files(root.as_path(), &["one.txt".to_string()])
            .expect("resolve upload file");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative, "one.txt");
        assert_eq!(files[0].bytes, 3);
        assert!(resolve_upload_files(root.as_path(), &[]).is_err());
        assert!(resolve_upload_files(root.as_path(), &["missing.txt".to_string()]).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn download_publish_never_overwrites_existing_target() {
        let root = test_workspace("download");
        let destination =
            prepare_download_destination(root.as_path(), "download.bin").expect("prepare download");
        fs::write(destination.staging.as_path(), b"payload").expect("write staging");
        fs::write(destination.target.as_path(), b"existing").expect("race target");
        assert!(publish_download(&destination).is_err());
        assert_eq!(
            fs::read(destination.target.as_path()).expect("read target"),
            b"existing"
        );
        remove_regular_file_or_symlink(destination.staging.as_path());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn download_publish_moves_verified_file_into_workspace() {
        let root = test_workspace("download_success");
        fs::create_dir(root.join("downloads")).expect("create downloads");
        let destination = prepare_download_destination(root.as_path(), "downloads/result.bin")
            .expect("prepare download");
        fs::write(destination.staging.as_path(), b"payload").expect("write staging");
        assert_eq!(publish_download(&destination).expect("publish"), 7);
        assert_eq!(
            fs::read(destination.target.as_path()).expect("read target"),
            b"payload"
        );
        assert!(!destination.staging.exists());
        let _ = fs::remove_dir_all(root);
    }
}
