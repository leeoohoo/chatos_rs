// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod filesystem;
mod transfer;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::relay::{relay_error_response, RelayRequest, RelayResponse};
use crate::skills::native::safe_workspace_path;
use crate::LocalState;

use super::runtime::{connect_session, RemoteConnectionSpec};
use filesystem::{
    create_directory, delete_entry, download_path, list_entries, read_remote_file, rename_entry,
    upload_path, write_remote_file,
};
pub(crate) use transfer::RemoteSftpManager;
use transfer::{TransferProgress, TransferStatus};

const DEFAULT_INLINE_READ_BYTES: usize = 256 * 1024;
const MAX_INLINE_READ_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct SftpBody {
    operation: String,
    connection_id: Option<String>,
    connection: Option<RemoteConnectionSpec>,
    verification_code: Option<String>,
    path: Option<String>,
    parent_path: Option<String>,
    name: Option<String>,
    from_path: Option<String>,
    to_path: Option<String>,
    recursive: Option<bool>,
    local_path: Option<String>,
    remote_path: Option<String>,
    direction: Option<String>,
    transfer_id: Option<String>,
    max_bytes: Option<usize>,
    content_base64: Option<String>,
    create_parent_dirs: Option<bool>,
    overwrite: Option<bool>,
}

pub(crate) async fn handle_remote_sftp_request(
    value: Value,
    state: &LocalState,
    manager: &RemoteSftpManager,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(error) => {
            return relay_error_response("remote_sftp_response", "", 400, error.to_string())
        }
    };
    let body = match serde_json::from_value::<SftpBody>(request.body.clone()) {
        Ok(body) => body,
        Err(error) => {
            return relay_error_response(
                "remote_sftp_response",
                request.request_id.as_str(),
                400,
                error.to_string(),
            )
        }
    };
    let result = match body.operation.as_str() {
        "transfer_status" => transfer_status(&body, manager).map(to_value),
        "transfer_cancel" => transfer_cancel(&body, manager).map(to_value),
        "transfer_start" => start_transfer(&request, body, state, manager)
            .await
            .map(to_value),
        _ => run_sftp_operation(&request, body, state).await,
    };
    match result {
        Ok(body) => RelayResponse {
            message_type: "remote_sftp_response".to_string(),
            request_id: request.request_id,
            status: 200,
            headers: BTreeMap::new(),
            body,
        }
        .into_value(),
        Err(error) => sftp_error_response(request.request_id, error),
    }
}

fn transfer_status(body: &SftpBody, manager: &RemoteSftpManager) -> Result<TransferStatus, String> {
    manager.status(
        required(&body.connection_id, "connection_id")?,
        required(&body.transfer_id, "transfer_id")?,
    )
}

fn transfer_cancel(body: &SftpBody, manager: &RemoteSftpManager) -> Result<TransferStatus, String> {
    manager.cancel(
        required(&body.connection_id, "connection_id")?,
        required(&body.transfer_id, "transfer_id")?,
    )
}

async fn run_sftp_operation(
    request: &RelayRequest,
    body: SftpBody,
    state: &LocalState,
) -> Result<Value, String> {
    let connection = body
        .connection
        .clone()
        .ok_or_else(|| "connection is required".to_string())?;
    let verification_code = body.verification_code.clone();
    let local_path = match body.operation.as_str() {
        "upload_path" | "download_path" => Some(resolve_local_path(
            state,
            request,
            required(&body.local_path, "local_path")?,
        )?),
        _ => None,
    };
    tokio::task::spawn_blocking(move || {
        let session = connect_session(
            &connection,
            Duration::from_secs(180),
            verification_code.as_deref(),
        )?;
        let sftp = session
            .sftp()
            .map_err(|error| format!("initialize SFTP failed: {error}"))?;
        match body.operation.as_str() {
            "list" => list_entries(&sftp, required(&body.path, "path")?),
            "mkdir" => create_directory(
                &sftp,
                required(&body.parent_path, "parent_path")?,
                required(&body.name, "name")?,
            ),
            "rename" => rename_entry(
                &sftp,
                required(&body.from_path, "from_path")?,
                required(&body.to_path, "to_path")?,
            ),
            "delete" => delete_entry(
                &sftp,
                required(&body.path, "path")?,
                body.recursive.unwrap_or(false),
            ),
            "read_file" => read_remote_file(
                &sftp,
                required(&body.remote_path, "remote_path")?,
                body.max_bytes
                    .unwrap_or(DEFAULT_INLINE_READ_BYTES)
                    .clamp(1, MAX_INLINE_READ_BYTES),
            ),
            "write_file" => write_remote_file(&sftp, &body),
            "upload_path" => upload_path(
                &sftp,
                local_path
                    .as_deref()
                    .ok_or_else(|| "local_path is required".to_string())?,
                required(&body.remote_path, "remote_path")?,
                None,
            )
            .map(|message| json!({ "success": true, "message": message })),
            "download_path" => download_path(
                &sftp,
                required(&body.remote_path, "remote_path")?,
                local_path
                    .as_deref()
                    .ok_or_else(|| "local_path is required".to_string())?,
                None,
            )
            .map(|message| json!({ "success": true, "message": message })),
            _ => Err(format!(
                "unsupported remote SFTP operation: {}",
                body.operation
            )),
        }
    })
    .await
    .map_err(|error| format!("remote SFTP worker failed: {error}"))?
}

async fn start_transfer(
    request: &RelayRequest,
    body: SftpBody,
    state: &LocalState,
    manager: &RemoteSftpManager,
) -> Result<TransferStatus, String> {
    let connection = body
        .connection
        .ok_or_else(|| "connection is required".to_string())?;
    let connection_id = required(&body.connection_id, "connection_id")?.to_string();
    let direction = required(&body.direction, "direction")?.to_string();
    if direction != "upload" && direction != "download" {
        return Err("direction must be upload or download".to_string());
    }
    let local_path = resolve_local_path(state, request, required(&body.local_path, "local_path")?)?;
    if direction == "upload" && !local_path.exists() {
        return Err("local upload path does not exist".to_string());
    }
    if direction == "download" {
        let parent = local_path
            .parent()
            .ok_or_else(|| "local target has no parent".to_string())?;
        if !parent.is_dir() {
            return Err("local target parent directory does not exist".to_string());
        }
    }
    let remote_path = required(&body.remote_path, "remote_path")?.to_string();
    let status = manager.create(connection_id, direction.clone(), Some(remote_path.clone()));
    let transfer_id = status.id.clone();
    let verification_code = body.verification_code;
    let manager_for_task = manager.clone();
    tokio::task::spawn_blocking(move || {
        manager_for_task.set_running(transfer_id.as_str());
        let progress = TransferProgress::new(transfer_id.clone(), manager_for_task.clone());
        let result = (|| {
            progress.check()?;
            let session = connect_session(
                &connection,
                Duration::from_secs(180),
                verification_code.as_deref(),
            )?;
            let sftp = session
                .sftp()
                .map_err(|error| format!("initialize SFTP failed: {error}"))?;
            if direction == "upload" {
                upload_path(
                    &sftp,
                    local_path.as_path(),
                    remote_path.as_str(),
                    Some(&progress),
                )
            } else {
                download_path(
                    &sftp,
                    remote_path.as_str(),
                    local_path.as_path(),
                    Some(&progress),
                )
            }
        })();
        match result {
            Ok(_) if manager_for_task.is_cancelled(transfer_id.as_str()) => {
                manager_for_task.mark_cancelled(transfer_id.as_str())
            }
            Ok(message) => manager_for_task.finish(transfer_id.as_str(), message),
            Err(_) if manager_for_task.is_cancelled(transfer_id.as_str()) => {
                manager_for_task.mark_cancelled(transfer_id.as_str())
            }
            Err(error) => manager_for_task.fail(transfer_id.as_str(), error),
        }
    });
    Ok(status)
}

fn resolve_local_path(
    state: &LocalState,
    request: &RelayRequest,
    path: &str,
) -> Result<PathBuf, String> {
    safe_workspace_path(state, request, path)
        .map(|(path, _)| path)
        .map_err(|error| error.to_string())
}

fn required<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str, String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{field} is required"))
}

fn to_value(value: TransferStatus) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn sftp_error_response(request_id: String, error: String) -> Value {
    let (status, body) = if let Some(prompt) = super::runtime::extract_second_factor_prompt(&error)
    {
        (
            400,
            json!({
                "error": "需要二次验证",
                "code": "second_factor_required",
                "challenge_prompt": prompt,
            }),
        )
    } else {
        let (status, code) = classify_sftp_error(error.as_str());
        (status, json!({ "error": error, "code": code }))
    };
    RelayResponse {
        message_type: "remote_sftp_response".to_string(),
        request_id,
        status,
        headers: BTreeMap::new(),
        body,
    }
    .into_value()
}

fn classify_sftp_error(error: &str) -> (u16, &'static str) {
    let normalized = error.to_lowercase();
    if normalized.contains("transfer not found") {
        return (404, "transfer_not_found");
    }
    if normalized.contains("transfer is not active") {
        return (409, "transfer_not_active");
    }
    if normalized.contains("transfer cancelled") {
        return (409, "transfer_cancelled");
    }
    if normalized.contains("authentication") || normalized.contains("认证失败") {
        return (401, "remote_auth_failed");
    }
    if normalized.contains("permission denied") || normalized.contains("sftp status 3") {
        return (403, "remote_permission_denied");
    }
    if normalized.contains("no such file") || normalized.contains("sftp status 2") {
        return (404, "remote_path_not_found");
    }
    if normalized.contains("timed out") || normalized.contains("超时") {
        return (408, "timeout");
    }
    if normalized.contains("connection refused")
        || normalized.contains("network is unreachable")
        || normalized.contains("connection reset")
        || normalized.contains("broken pipe")
    {
        return (502, "remote_network_disconnected");
    }
    if normalized.contains("local upload path")
        || normalized.contains("local target")
        || normalized.contains("symbolic link")
        || normalized.contains("path must be")
        || normalized.contains("already exists")
        || normalized.contains("remote root")
    {
        return (400, "invalid_path");
    }
    if normalized.contains("local file") || normalized.contains("local directory") {
        return (500, "local_io_error");
    }
    if normalized.contains("required")
        || normalized.contains("direction must")
        || normalized.contains("invalid remote directory name")
        || normalized.contains("unsupported remote sftp operation")
    {
        return (400, "invalid_argument");
    }
    (400, "remote_error")
}

#[cfg(test)]
mod tests {
    use super::classify_sftp_error;

    #[test]
    fn classifies_transfer_and_path_errors_for_the_sftp_api() {
        assert_eq!(
            classify_sftp_error("transfer not found"),
            (404, "transfer_not_found")
        );
        assert_eq!(
            classify_sftp_error("stat remote entry failed: SFTP status 2"),
            (404, "remote_path_not_found")
        );
        assert_eq!(
            classify_sftp_error("local upload path does not exist"),
            (400, "invalid_path")
        );
    }
}
