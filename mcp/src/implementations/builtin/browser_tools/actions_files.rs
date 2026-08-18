// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use super::actions_shared::{
    copy_response_fields, fail_json, finalize_browser_action_response, is_success, normalize_ref,
    parse_browser_eval_payload, run_basic_browser_action, run_browser_command,
};
use super::BoundContext;

pub(super) const MAX_BROWSER_UPLOAD_FILES: usize = 10;
pub(super) const MAX_BROWSER_UPLOAD_FILE_BYTES: u64 = 50 * 1024 * 1024;
pub(super) const MAX_BROWSER_UPLOAD_TOTAL_BYTES: u64 = 100 * 1024 * 1024;
pub(super) const MAX_BROWSER_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024;
const MAX_BROWSER_FILE_PATH_CHARS: usize = 4_096;
const DOWNLOAD_STAGING_PREFIX: &str = ".chatos-browser-download-";
const BLOB_DOWNLOAD_CHUNK_BYTES: u64 = 512 * 1024;
const DOWNLOAD_TEXT_PREVIEW_BYTES: usize = 8 * 1024;

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

#[derive(Debug)]
struct BrowserDownloadEvidence {
    bytes: u64,
    sha256: String,
    suggested_filename: Option<String>,
    mime_type: String,
    source_kind: String,
    content_preview: Option<String>,
    json_content: Option<Value>,
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
    let href = browser_element_attribute(&ctx, session.as_str(), reference.as_str(), "href").await;
    let suggested_filename =
        browser_element_attribute(&ctx, session.as_str(), reference.as_str(), "download").await;
    let (result, evidence) = if href.is_none() {
        match capture_generated_blob_download(
            &ctx,
            session.as_str(),
            reference.as_str(),
            &destination,
        )
        .await
        {
            Ok((result, evidence)) => (result, evidence),
            Err(error) => {
                remove_regular_file_or_symlink(destination.staging.as_path());
                remove_regular_file_or_symlink(destination.target.as_path());
                return Ok(json!({
                    "_summary_text": format!("Browser download failed: {error}."),
                    "success": false,
                    "error": error,
                    "path": destination.relative,
                }));
            }
        }
    } else {
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
        let evidence = publish_download_with_evidence(
            &destination,
            suggested_filename,
            href.as_deref(),
            None,
            "browser_download_event",
        );
        let evidence = match evidence {
            Ok(evidence) => evidence,
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
        (result, evidence)
    };

    Ok(finalize_browser_action_response(
        &ctx,
        session.as_str(),
        json!({
            "success": true,
            "element": reference,
            "path": destination.relative,
            "bytes": evidence.bytes,
            "sha256": evidence.sha256,
            "suggested_filename": evidence.suggested_filename,
            "mime_type": evidence.mime_type,
            "source_kind": evidence.source_kind,
            "content_preview": evidence.content_preview,
            "json_content": evidence.json_content,
            "overwrote_existing": false,
            "browser_session": result.get("browser_session").cloned(),
        }),
        "Downloaded and verified a browser file in the workspace without overwriting an existing path.",
        Some("The result includes the browser filename, MIME type, byte count, SHA-256, and bounded content evidence."),
    )
    .await)
}

async fn browser_element_attribute(
    ctx: &BoundContext,
    session: &str,
    reference: &str,
    attribute: &str,
) -> Option<String> {
    let result = run_browser_command(
        ctx,
        session,
        "get",
        vec![
            "attr".to_string(),
            reference.to_string(),
            attribute.to_string(),
        ],
        ctx.command_timeout_seconds,
    )
    .await
    .ok()?;
    if !is_success(&result) {
        return None;
    }
    result
        .pointer("/data/value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn capture_generated_blob_download(
    ctx: &BoundContext,
    session: &str,
    reference: &str,
    destination: &DownloadDestination,
) -> Result<(Value, BrowserDownloadEvidence), String> {
    let installed = run_browser_command(
        ctx,
        session,
        "eval",
        vec![blob_capture_install_script().to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&installed) {
        return Err("installing the Blob download capture hook failed".to_string());
    }
    let click = run_browser_command(
        ctx,
        session,
        "click",
        vec![reference.to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&click) {
        return Err("clicking the export control failed".to_string());
    }

    let mut metadata = None;
    for _ in 0..20 {
        let value = run_browser_command(
            ctx,
            session,
            "eval",
            vec![blob_capture_metadata_script().to_string()],
            ctx.command_timeout_seconds,
        )
        .await?;
        if is_success(&value) {
            let parsed = parse_browser_eval_payload(
                value
                    .pointer("/data/result")
                    .cloned()
                    .unwrap_or(Value::Null),
            );
            if parsed.get("ready").and_then(Value::as_bool) == Some(true) {
                metadata = Some(parsed);
                break;
            }
        }
        sleep(Duration::from_millis(50)).await;
    }
    let metadata = metadata.ok_or_else(|| {
        "the clicked control did not expose a capturable Blob/data download".to_string()
    })?;
    let bytes = metadata
        .get("size")
        .and_then(Value::as_u64)
        .ok_or_else(|| "captured browser download is missing its byte size".to_string())?;
    if bytes > MAX_BROWSER_DOWNLOAD_BYTES {
        return Err(format!(
            "browser download exceeds {MAX_BROWSER_DOWNLOAD_BYTES} bytes"
        ));
    }
    let suggested_filename = metadata
        .get("filename")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let mime_type = metadata
        .get("mimeType")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream")
        .to_string();
    let source_kind = metadata
        .get("sourceKind")
        .and_then(Value::as_str)
        .unwrap_or("blob")
        .to_string();

    let mut staging = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination.staging.as_path())
        .map_err(|error| format!("create Blob download staging file failed: {error}"))?;
    let mut hasher = Sha256::new();
    let mut written = 0_u64;
    while written < bytes {
        let end = written.saturating_add(BLOB_DOWNLOAD_CHUNK_BYTES).min(bytes);
        let value = run_browser_command(
            ctx,
            session,
            "eval",
            vec![blob_capture_chunk_script(written, end)],
            ctx.command_timeout_seconds,
        )
        .await?;
        if !is_success(&value) {
            return Err(format!(
                "reading Blob download bytes {written}..{end} failed"
            ));
        }
        let parsed = parse_browser_eval_payload(
            value
                .pointer("/data/result")
                .cloned()
                .unwrap_or(Value::Null),
        );
        let encoded = parsed
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| "Blob download chunk is missing base64 data".to_string())?;
        let chunk = STANDARD
            .decode(encoded)
            .map_err(|error| format!("decode Blob download chunk failed: {error}"))?;
        if u64::try_from(chunk.len()).ok() != Some(end - written) {
            return Err("Blob download chunk size verification failed".to_string());
        }
        staging
            .write_all(chunk.as_slice())
            .map_err(|error| format!("write Blob download staging file failed: {error}"))?;
        hasher.update(chunk.as_slice());
        written = end;
    }
    staging
        .flush()
        .map_err(|error| format!("flush Blob download staging file failed: {error}"))?;
    staging
        .sync_all()
        .map_err(|error| format!("sync Blob download staging file failed: {error}"))?;
    drop(staging);
    let evidence = publish_download_with_evidence(
        destination,
        suggested_filename,
        None,
        Some(mime_type),
        source_kind.as_str(),
    )?;
    if evidence.sha256 != hex::encode(hasher.finalize()) {
        remove_regular_file_or_symlink(destination.target.as_path());
        return Err("Blob download SHA-256 changed while publishing".to_string());
    }
    let _ = run_browser_command(
        ctx,
        session,
        "eval",
        vec!["(()=>{if(window.__chatosDownloadCaptureV1){window.__chatosDownloadCaptureV1.blob=null}return true})()".to_string()],
        ctx.command_timeout_seconds,
    )
    .await;
    Ok((click, evidence))
}

fn blob_capture_install_script() -> &'static str {
    r#"(()=>{const key="__chatosDownloadCaptureV1";const state=window[key]||{installed:false};state.blob=null;state.filename=null;state.sourceKind=null;state.error=null;window[key]=state;if(state.installed)return JSON.stringify({installed:true,reused:true});const blobs=new Map();const create=URL.createObjectURL.bind(URL);const revoke=URL.revokeObjectURL.bind(URL);URL.createObjectURL=(value)=>{const url=create(value);if(value instanceof Blob)blobs.set(url,value);return url};URL.revokeObjectURL=(url)=>{setTimeout(()=>{blobs.delete(url);revoke(url)},30000)};const capture=(anchor)=>{const href=anchor.href||"";const current=window[key];const blob=blobs.get(href);if(blob){current.blob=blob;current.filename=anchor.download||"download";current.sourceKind="blob";return true}if(href.startsWith("data:")){fetch(href).then(response=>response.blob()).then(value=>{current.blob=value;current.filename=anchor.download||"download";current.sourceKind="data"}).catch(error=>{current.error=String(error)});return true}return false};const nativeClick=HTMLAnchorElement.prototype.click;HTMLAnchorElement.prototype.click=function(){if(capture(this))return;return nativeClick.call(this)};document.addEventListener("click",event=>{const anchor=event.target instanceof Element?event.target.closest("a[href]"):null;if(anchor&&capture(anchor)){event.preventDefault();event.stopImmediatePropagation()}},true);state.installed=true;return JSON.stringify({installed:true,reused:false})})()"#
}

fn blob_capture_metadata_script() -> &'static str {
    r#"(()=>{const state=window.__chatosDownloadCaptureV1;return JSON.stringify({ready:!!state?.blob,filename:state?.filename??null,mimeType:state?.blob?.type||"application/octet-stream",size:state?.blob?.size??null,sourceKind:state?.sourceKind??null,error:state?.error??null})})()"#
}

fn blob_capture_chunk_script(start: u64, end: u64) -> String {
    format!(
        r#"(async()=>{{const blob=window.__chatosDownloadCaptureV1?.blob;if(!blob)throw new Error("captured Blob is unavailable");const bytes=new Uint8Array(await blob.slice({start},{end}).arrayBuffer());let binary="";for(let index=0;index<bytes.length;index+=32768)binary+=String.fromCharCode(...bytes.subarray(index,index+32768));return JSON.stringify({{data:btoa(binary),bytes:bytes.length}})}})()"#
    )
}

fn publish_download_with_evidence(
    destination: &DownloadDestination,
    suggested_filename: Option<String>,
    source_url: Option<&str>,
    explicit_mime_type: Option<String>,
    source_kind: &str,
) -> Result<BrowserDownloadEvidence, String> {
    let bytes = publish_download(destination)?;
    let content = fs::read(destination.target.as_path())
        .map_err(|error| format!("read verified browser download failed: {error}"))?;
    if u64::try_from(content.len()).ok() != Some(bytes) {
        return Err("verified browser download size changed after publishing".to_string());
    }
    let sha256 = hex::encode(Sha256::digest(content.as_slice()));
    let mime_type = explicit_mime_type
        .filter(|value| !value.trim().is_empty())
        .or_else(|| source_url.and_then(mime_type_from_data_url))
        .unwrap_or_else(|| infer_download_mime_type(destination, content.as_slice()));
    let textual = mime_type.starts_with("text/")
        || matches!(
            mime_type.as_str(),
            "application/json" | "application/xml" | "application/javascript"
        );
    let content_preview = textual
        .then(|| {
            std::str::from_utf8(&content[..content.len().min(DOWNLOAD_TEXT_PREVIEW_BYTES)])
                .ok()
                .map(ToOwned::to_owned)
        })
        .flatten();
    let json_content = (mime_type == "application/json")
        .then(|| serde_json::from_slice(content.as_slice()).ok())
        .flatten();
    Ok(BrowserDownloadEvidence {
        bytes,
        sha256,
        suggested_filename,
        mime_type,
        source_kind: source_kind.to_string(),
        content_preview,
        json_content,
    })
}

fn mime_type_from_data_url(url: &str) -> Option<String> {
    let metadata = url.strip_prefix("data:")?.split_once(',')?.0;
    let mime = metadata.split(';').next()?.trim();
    (!mime.is_empty()).then(|| mime.to_string())
}

fn infer_download_mime_type(destination: &DownloadDestination, content: &[u8]) -> String {
    if serde_json::from_slice::<Value>(content).is_ok() {
        return "application/json".to_string();
    }
    match destination
        .target
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "txt" | "md" | "csv" => "text/plain".to_string(),
        "json" => "application/json".to_string(),
        "pdf" => "application/pdf".to_string(),
        "png" => "image/png".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "webp" => "image/webp".to_string(),
        _ => "application/octet-stream".to_string(),
    }
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
