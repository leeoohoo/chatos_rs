use super::{
    ssh::{
        connection_fingerprint, is_reparse_point, user_facing_failure, NativeOpenSshTester,
        ProcessOutcome, ResetFuture, SshConnectionMaterial, SshProcessInput, SshProcessOutput,
        SshProcessProgress, INTERACTIVE_SESSION_TIMEOUT,
    },
    RemoteConnectionManager, RemoteConnectionMetadataClient,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Mutex as StandardMutex,
    time::{Duration, Instant},
};
use tauri::State;

const REMOTE_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const MAXIMUM_REMOTE_PATH_BYTES: usize = 4_096;
const MAXIMUM_REMOTE_NAME_BYTES: usize = 1_024;
const MAXIMUM_DIRECTORY_ENTRIES: usize = 1_000;
const MAXIMUM_DIRECTORY_OUTPUT_BYTES: usize = 2 * 1_024 * 1_024;
const LISTING_MARKER: &str = "__CHATOS_REMOTE_LIST_V1__";
const COMMITTED_MARKER: &str = "__CHATOS_REMOTE_MUTATION_COMMITTED_V1__";
const TRUNCATED_MARKER: &str = "__CHATOS_REMOTE_LIST_TRUNCATED_V1__";
const DOWNLOAD_SIZE_MARKER: &str = "__CHATOS_REMOTE_DOWNLOAD_SIZE_V1__";
const MAXIMUM_TRANSFER_BYTES: u64 = 8 * 1_024 * 1_024 * 1_024;
const MAXIMUM_PENDING_SELECTIONS: usize = 32;
const SELECTION_LIFETIME: Duration = Duration::from_secs(15 * 60);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const TRANSFER_PROCESS_LIFETIME: Duration = Duration::from_secs(65 * 60);

type FileFuture<'a> = Pin<Box<dyn Future<Output = Result<SshProcessProgress, String>> + Send + 'a>>;

pub(super) trait RemoteFileExecutor: Send + Sync {
    fn execute(
        &self,
        key: String,
        fingerprint: String,
        connection: SshConnectionMaterial,
        command: String,
        verification_code: Option<String>,
        request_timeout: Duration,
        process_lifetime: Duration,
        input: SshProcessInput,
        output: SshProcessOutput,
    ) -> FileFuture<'_>;

    fn reset(&self) -> ResetFuture<'_>;
}

impl RemoteFileExecutor for NativeOpenSshTester {
    fn execute(
        &self,
        key: String,
        fingerprint: String,
        connection: SshConnectionMaterial,
        command: String,
        verification_code: Option<String>,
        request_timeout: Duration,
        process_lifetime: Duration,
        input: SshProcessInput,
        output: SshProcessOutput,
    ) -> FileFuture<'_> {
        Box::pin(self.run_remote_command(
            key,
            fingerprint,
            connection,
            command,
            verification_code,
            request_timeout,
            process_lifetime,
            input,
            output,
        ))
    }

    fn reset(&self) -> ResetFuture<'_> {
        <Self as super::ssh::RemoteConnectionTester>::reset(self)
    }
}

#[cfg(test)]
pub(super) struct DisabledRemoteFileExecutor;

#[cfg(test)]
impl RemoteFileExecutor for DisabledRemoteFileExecutor {
    fn execute(
        &self,
        _key: String,
        _fingerprint: String,
        _connection: SshConnectionMaterial,
        _command: String,
        _verification_code: Option<String>,
        _request_timeout: Duration,
        _process_lifetime: Duration,
        _input: SshProcessInput,
        _output: SshProcessOutput,
    ) -> FileFuture<'_> {
        Box::pin(async { Err("远端文件执行器未配置。".into()) })
    }

    fn reset(&self) -> ResetFuture<'_> {
        Box::pin(async {})
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteFileOperationInput {
    operation_id: String,
    connection_id: String,
    #[serde(default)]
    verification_code: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalSelectionKind {
    Upload,
    Download,
}

#[derive(Clone, Debug)]
struct PendingLocalSelection {
    owner_user_id: String,
    presentation_id: String,
    kind: LocalSelectionKind,
    path: PathBuf,
    file_name: String,
    size: Option<u64>,
    created_at: Instant,
}

#[derive(Default)]
pub(super) struct RemoteFileSelectionStore {
    selections: StandardMutex<HashMap<String, PendingLocalSelection>>,
}

impl RemoteFileSelectionStore {
    fn stage(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        kind: LocalSelectionKind,
        path: PathBuf,
        file_name: String,
        size: Option<u64>,
    ) -> Result<PresentedRemoteLocalFileSelection, String> {
        validate_presentation_id(presentation_id)?;
        let mut selections = self
            .selections
            .lock()
            .map_err(|_| local_selection_error())?;
        selections.retain(|_, selection| selection.created_at.elapsed() <= SELECTION_LIFETIME);
        if selections.len() >= MAXIMUM_PENDING_SELECTIONS {
            return Err("待处理的远端文件选择过多，请关闭其他远端文件窗口后重试。".into());
        }
        let selection_id = format!("remote-file-selection-{}", uuid::Uuid::new_v4());
        selections.insert(
            selection_id.clone(),
            PendingLocalSelection {
                owner_user_id: owner_user_id.to_owned(),
                presentation_id: presentation_id.to_owned(),
                kind,
                path,
                file_name: file_name.clone(),
                size,
                created_at: Instant::now(),
            },
        );
        Ok(PresentedRemoteLocalFileSelection {
            selection_id,
            file_name,
            size,
        })
    }

    fn resolve(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        selection_id: &str,
        kind: LocalSelectionKind,
    ) -> Result<PendingLocalSelection, String> {
        validate_presentation_id(presentation_id)?;
        validate_selection_id(selection_id)?;
        let mut selections = self
            .selections
            .lock()
            .map_err(|_| local_selection_error())?;
        selections.retain(|_, selection| selection.created_at.elapsed() <= SELECTION_LIFETIME);
        let selection = selections
            .get(selection_id)
            .filter(|selection| {
                selection.owner_user_id == owner_user_id
                    && selection.presentation_id == presentation_id
                    && selection.kind == kind
            })
            .ok_or_else(selection_expired)?;
        Ok(selection.clone())
    }

    fn consume(&self, selection_id: &str) {
        if let Ok(mut selections) = self.selections.lock() {
            selections.remove(selection_id);
        }
    }

    fn discard(&self, owner_user_id: &str, presentation_id: &str) {
        if let Ok(mut selections) = self.selections.lock() {
            selections.retain(|_, selection| {
                selection.owner_user_id != owner_user_id
                    || selection.presentation_id != presentation_id
            });
        }
    }

    pub(super) fn reset(&self) {
        if let Ok(mut selections) = self.selections.lock() {
            selections.clear();
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedRemoteLocalFileSelection {
    selection_id: String,
    file_name: String,
    size: Option<u64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PresentedRemoteDownloadResponse {
    Completed { file_name: String, size: u64 },
    VerificationRequired { prompt: String },
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresentedRemoteFileKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedRemoteFileEntry {
    name: String,
    path: String,
    kind: PresentedRemoteFileKind,
    size: Option<u64>,
    modified_at_epoch_seconds: Option<i64>,
    permissions: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedRemoteDirectoryListing {
    path: String,
    parent_path: Option<String>,
    entries: Vec<PresentedRemoteFileEntry>,
    truncated: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PresentedRemoteDirectoryResult {
    Completed {
        listing: PresentedRemoteDirectoryListing,
    },
    VerificationRequired {
        prompt: String,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedRemoteFileMutationResult {
    committed: bool,
    refreshed_listing: Option<PresentedRemoteDirectoryListing>,
    message: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PresentedRemoteFileMutationResponse {
    Completed {
        result: PresentedRemoteFileMutationResult,
    },
    VerificationRequired {
        prompt: String,
    },
}

impl RemoteConnectionManager {
    async fn execute_remote_file_listing(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        input: &RemoteFileOperationInput,
        requested_path: Option<String>,
    ) -> Result<SshProcessProgress, String> {
        validate_operation_id(&input.operation_id)?;
        let _mutation = self.mutation.lock().await;
        if input.verification_code.is_none() {
            self.file_executor.reset().await;
        }
        let material = self
            .saved_material(client, owner_user_id, &input.connection_id)
            .await?;
        let path = match requested_path {
            Some(path) => validate_remote_path(&path)?,
            None => {
                validate_remote_path(material.spec.default_remote_path.as_deref().unwrap_or("."))?
            }
        };
        let operation = RemoteFileOperation {
            identity: format!("list\0{path}"),
            command: listing_script(&path),
        };
        let fingerprint = operation_fingerprint(&material, &operation);
        self.file_executor
            .execute(
                format!("remote-file:{}", input.operation_id),
                fingerprint,
                material,
                operation.command,
                input.verification_code.clone(),
                REMOTE_OPERATION_TIMEOUT,
                INTERACTIVE_SESSION_TIMEOUT,
                SshProcessInput::None,
                SshProcessOutput::Capture {
                    maximum_bytes: MAXIMUM_DIRECTORY_OUTPUT_BYTES,
                },
            )
            .await
    }

    async fn execute_remote_file_operation(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        input: &RemoteFileOperationInput,
        operation: &RemoteFileOperation,
    ) -> Result<SshProcessProgress, String> {
        validate_operation_id(&input.operation_id)?;
        let _mutation = self.mutation.lock().await;
        if input.verification_code.is_none() {
            self.file_executor.reset().await;
        }
        let material = self
            .saved_material(client, owner_user_id, &input.connection_id)
            .await?;
        let fingerprint = operation_fingerprint(&material, operation);
        self.file_executor
            .execute(
                format!("remote-file:{}", input.operation_id),
                fingerprint,
                material,
                operation.command.clone(),
                input.verification_code.clone(),
                REMOTE_OPERATION_TIMEOUT,
                INTERACTIVE_SESSION_TIMEOUT,
                SshProcessInput::None,
                SshProcessOutput::Capture {
                    maximum_bytes: MAXIMUM_DIRECTORY_OUTPUT_BYTES,
                },
            )
            .await
    }

    async fn execute_remote_file_transfer(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        input: &RemoteFileOperationInput,
        operation: &RemoteFileOperation,
        process_input: SshProcessInput,
        process_output: SshProcessOutput,
    ) -> Result<SshProcessProgress, String> {
        validate_operation_id(&input.operation_id)?;
        let _mutation = self.mutation.lock().await;
        if input.verification_code.is_none() {
            self.file_executor.reset().await;
        }
        let material = self
            .saved_material(client, owner_user_id, &input.connection_id)
            .await?;
        let fingerprint = operation_fingerprint(&material, operation);
        self.file_executor
            .execute(
                format!("remote-file:{}", input.operation_id),
                fingerprint,
                material,
                operation.command.clone(),
                input.verification_code.clone(),
                TRANSFER_TIMEOUT,
                TRANSFER_PROCESS_LIFETIME,
                process_input,
                process_output,
            )
            .await
    }
}

#[derive(Clone)]
struct RemoteFileOperation {
    identity: String,
    command: String,
}

#[tauri::command]
pub async fn pick_remote_upload_file(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
) -> Result<Option<PresentedRemoteLocalFileSelection>, String> {
    crate::host_lifecycle::validate_owner(&owner_user_id)?;
    validate_presentation_id(&presentation_id)?;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title("选择要上传的文件")
        .pick_file()
        .await;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let requested = selected.path().to_path_buf();
    let (path, file_name, size) =
        tauri::async_runtime::spawn_blocking(move || prepare_upload_file(&requested))
            .await
            .map_err(|_| local_selection_error())??;
    manager
        .file_selections
        .stage(
            &owner_user_id,
            &presentation_id,
            LocalSelectionKind::Upload,
            path,
            file_name,
            Some(size),
        )
        .map(Some)
}

#[tauri::command]
pub async fn pick_remote_download_destination(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    suggested_file_name: String,
) -> Result<Option<PresentedRemoteLocalFileSelection>, String> {
    crate::host_lifecycle::validate_owner(&owner_user_id)?;
    validate_presentation_id(&presentation_id)?;
    let suggested_file_name = validate_local_file_name(&suggested_file_name)?;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title("保存远端文件")
        .set_file_name(&suggested_file_name)
        .save_file()
        .await;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let requested = selected.path().to_path_buf();
    let (path, file_name) =
        tauri::async_runtime::spawn_blocking(move || prepare_download_destination(&requested))
            .await
            .map_err(|_| local_selection_error())??;
    manager
        .file_selections
        .stage(
            &owner_user_id,
            &presentation_id,
            LocalSelectionKind::Download,
            path,
            file_name,
            None,
        )
        .map(Some)
}

#[tauri::command]
pub async fn discard_remote_file_selections(
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
) -> Result<(), String> {
    crate::host_lifecycle::validate_owner(&owner_user_id)?;
    validate_presentation_id(&presentation_id)?;
    manager.file_executor.reset().await;
    manager
        .file_selections
        .discard(&owner_user_id, &presentation_id);
    Ok(())
}

#[tauri::command]
pub async fn list_remote_directory(
    host: State<'_, crate::host_lifecycle::HostManager>,
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    input: RemoteFileOperationInput,
    path: Option<String>,
) -> Result<PresentedRemoteDirectoryResult, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    match manager
        .execute_remote_file_listing(&*host, &owner_user_id, &input, path)
        .await?
    {
        SshProcessProgress::VerificationRequired(prompt) => {
            Ok(PresentedRemoteDirectoryResult::VerificationRequired { prompt })
        }
        SshProcessProgress::Completed(outcome) => Ok(PresentedRemoteDirectoryResult::Completed {
            listing: finish_listing(outcome)?,
        }),
    }
}

#[tauri::command]
pub async fn create_remote_directory(
    host: State<'_, crate::host_lifecycle::HostManager>,
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    input: RemoteFileOperationInput,
    parent_path: String,
    name: String,
) -> Result<PresentedRemoteFileMutationResponse, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let parent_path = validate_remote_path(&parent_path)?;
    let name = validate_remote_name(&name)?;
    let mutation = format!("mkdir -- {}", shell_quote(&name));
    execute_mutation(
        &*host,
        &manager,
        &owner_user_id,
        input,
        "create_directory",
        &parent_path,
        &mutation,
    )
    .await
}

#[tauri::command]
pub async fn rename_remote_entry(
    host: State<'_, crate::host_lifecycle::HostManager>,
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    input: RemoteFileOperationInput,
    path: String,
    new_name: String,
) -> Result<PresentedRemoteFileMutationResponse, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let path = validate_mutable_remote_path(&path)?;
    let new_name = validate_remote_name(&new_name)?;
    let (parent, name) = split_remote_path(&path)?;
    let mutation = format!("mv -- {} {}", shell_quote(&name), shell_quote(&new_name));
    execute_mutation(
        &*host,
        &manager,
        &owner_user_id,
        input,
        "rename",
        &parent,
        &mutation,
    )
    .await
}

#[tauri::command]
pub async fn delete_remote_entry(
    host: State<'_, crate::host_lifecycle::HostManager>,
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    input: RemoteFileOperationInput,
    path: String,
    recursively: bool,
) -> Result<PresentedRemoteFileMutationResponse, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let path = validate_mutable_remote_path(&path)?;
    let (parent, name) = split_remote_path(&path)?;
    let mutation = if recursively {
        format!("rm -rf -- {}", shell_quote(&name))
    } else {
        format!("rm -- {}", shell_quote(&name))
    };
    execute_mutation(
        &*host,
        &manager,
        &owner_user_id,
        input,
        if recursively {
            "delete_recursive"
        } else {
            "delete"
        },
        &parent,
        &mutation,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn upload_remote_file(
    host: State<'_, crate::host_lifecycle::HostManager>,
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    input: RemoteFileOperationInput,
    selection_id: String,
    remote_directory: String,
    overwrite: bool,
) -> Result<PresentedRemoteFileMutationResponse, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let selection = manager.file_selections.resolve(
        &owner_user_id,
        &presentation_id,
        &selection_id,
        LocalSelectionKind::Upload,
    )?;
    let (local_path, size) = match revalidate_upload_file(&selection) {
        Ok(value) => value,
        Err(error) => {
            manager.file_executor.reset().await;
            return Err(error);
        }
    };
    let remote_directory = validate_remote_path(&remote_directory)?;
    let remote_name = validate_remote_name(&selection.file_name)?;
    let mutation = upload_script(&remote_name, size, overwrite);
    let operation = RemoteFileOperation {
        identity: format!(
            "upload\0{selection_id}\0{remote_directory}\0{remote_name}\0{size}\0{overwrite}"
        ),
        command: mutation_and_listing_script(&remote_directory, &mutation),
    };
    let progress = manager
        .execute_remote_file_transfer(
            &*host,
            &owner_user_id,
            &input,
            &operation,
            SshProcessInput::File {
                path: local_path,
                maximum_bytes: MAXIMUM_TRANSFER_BYTES,
                expected_bytes: size,
            },
            SshProcessOutput::Capture {
                maximum_bytes: MAXIMUM_DIRECTORY_OUTPUT_BYTES,
            },
        )
        .await?;
    match progress {
        SshProcessProgress::VerificationRequired(prompt) => {
            Ok(PresentedRemoteFileMutationResponse::VerificationRequired { prompt })
        }
        SshProcessProgress::Completed(outcome) => {
            let result = finish_mutation(outcome)?;
            if result.committed {
                manager.file_selections.consume(&selection_id);
            }
            Ok(PresentedRemoteFileMutationResponse::Completed { result })
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn download_remote_file(
    host: State<'_, crate::host_lifecycle::HostManager>,
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    input: RemoteFileOperationInput,
    selection_id: String,
    remote_path: String,
    overwrite: bool,
) -> Result<PresentedRemoteDownloadResponse, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let selection = manager.file_selections.resolve(
        &owner_user_id,
        &presentation_id,
        &selection_id,
        LocalSelectionKind::Download,
    )?;
    let destination = match revalidate_download_destination(&selection, overwrite) {
        Ok(value) => value,
        Err(error) => {
            manager.file_executor.reset().await;
            return Err(error);
        }
    };
    let remote_path = validate_mutable_remote_path(&remote_path)?;
    let temporary_path = download_temporary_path(&destination, &selection_id, &input.operation_id)?;
    let operation = RemoteFileOperation {
        identity: format!("download\0{selection_id}\0{remote_path}\0{overwrite}"),
        command: download_script(&remote_path),
    };
    let progress = manager
        .execute_remote_file_transfer(
            &*host,
            &owner_user_id,
            &input,
            &operation,
            SshProcessInput::None,
            SshProcessOutput::File {
                path: temporary_path,
                maximum_bytes: MAXIMUM_TRANSFER_BYTES,
            },
        )
        .await?;
    match progress {
        SshProcessProgress::VerificationRequired(prompt) => {
            Ok(PresentedRemoteDownloadResponse::VerificationRequired { prompt })
        }
        SshProcessProgress::Completed(mut outcome) => {
            let size = finish_download(&mut outcome, &destination, overwrite)?;
            manager.file_selections.consume(&selection_id);
            Ok(PresentedRemoteDownloadResponse::Completed {
                file_name: selection.file_name,
                size,
            })
        }
    }
}

async fn execute_mutation(
    client: &dyn RemoteConnectionMetadataClient,
    manager: &RemoteConnectionManager,
    owner_user_id: &str,
    input: RemoteFileOperationInput,
    kind: &str,
    parent_path: &str,
    mutation: &str,
) -> Result<PresentedRemoteFileMutationResponse, String> {
    let operation = RemoteFileOperation {
        identity: format!("{kind}\0{parent_path}\0{mutation}"),
        command: mutation_and_listing_script(parent_path, mutation),
    };
    match manager
        .execute_remote_file_operation(client, owner_user_id, &input, &operation)
        .await?
    {
        SshProcessProgress::VerificationRequired(prompt) => {
            Ok(PresentedRemoteFileMutationResponse::VerificationRequired { prompt })
        }
        SshProcessProgress::Completed(outcome) => {
            Ok(PresentedRemoteFileMutationResponse::Completed {
                result: finish_mutation(outcome)?,
            })
        }
    }
}

fn operation_fingerprint(
    material: &SshConnectionMaterial,
    operation: &RemoteFileOperation,
) -> String {
    let mut digest = Sha256::new();
    digest.update(connection_fingerprint(material));
    digest.update([0]);
    digest.update(operation.identity.as_bytes());
    format!("{:x}", digest.finalize())
}

fn finish_listing(outcome: ProcessOutcome) -> Result<PresentedRemoteDirectoryListing, String> {
    if outcome.timed_out {
        return Err("远端目录读取超时。".into());
    }
    if outcome.output_truncated {
        return Err("远端目录响应超过安全上限。".into());
    }
    if outcome.exit_code != 0 {
        return Err(remote_file_failure(&outcome.stderr));
    }
    parse_listing(&outcome.stdout)
}

fn finish_mutation(outcome: ProcessOutcome) -> Result<PresentedRemoteFileMutationResult, String> {
    let committed = has_nul_field(&outcome.stdout, COMMITTED_MARKER.as_bytes());
    if !committed {
        if outcome.timed_out {
            return Err("远端文件操作超时，未确认是否提交。请刷新目录后再操作。".into());
        }
        if outcome.output_truncated {
            return Err("远端文件操作响应超过安全上限。".into());
        }
        if outcome.exit_code != 0 {
            return Err(remote_file_failure(&outcome.stderr));
        }
        return Err("远端文件操作没有返回有效的提交确认。".into());
    }
    let refreshed_listing = if !outcome.timed_out && !outcome.output_truncated {
        parse_listing_after_marker(&outcome.stdout).ok()
    } else {
        None
    };
    Ok(PresentedRemoteFileMutationResult {
        committed: true,
        message: refreshed_listing
            .is_none()
            .then(|| "远端操作已完成，但目录刷新失败，请手动刷新。".into()),
        refreshed_listing,
    })
}

fn parse_listing_after_marker(bytes: &[u8]) -> Result<PresentedRemoteDirectoryListing, String> {
    let fields = nul_fields(bytes);
    let marker_index = fields
        .iter()
        .position(|field| *field == COMMITTED_MARKER.as_bytes())
        .ok_or_else(remote_response_error)?;
    let remaining = fields
        .get(marker_index + 1..)
        .ok_or_else(remote_response_error)?;
    parse_listing_fields(remaining)
}

fn parse_listing(bytes: &[u8]) -> Result<PresentedRemoteDirectoryListing, String> {
    let fields = nul_fields(bytes);
    parse_listing_fields(&fields)
}

fn parse_listing_fields(fields: &[&[u8]]) -> Result<PresentedRemoteDirectoryListing, String> {
    let marker_index = fields
        .iter()
        .position(|field| *field == LISTING_MARKER.as_bytes())
        .ok_or_else(remote_response_error)?;
    let fields = fields
        .get(marker_index..)
        .ok_or_else(remote_response_error)?;
    let path = fields
        .get(1)
        .and_then(|field| std::str::from_utf8(field).ok())
        .ok_or_else(remote_response_error)
        .and_then(validate_resolved_remote_path)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut index = 2;
    while index < fields.len() {
        if fields[index] == TRUNCATED_MARKER.as_bytes() {
            truncated = true;
            index += 1;
            break;
        }
        if index + 4 >= fields.len() || entries.len() >= MAXIMUM_DIRECTORY_ENTRIES {
            return Err(remote_response_error());
        }
        let kind = match utf8_field(fields[index])? {
            "directory" => PresentedRemoteFileKind::Directory,
            "file" => PresentedRemoteFileKind::File,
            "symlink" => PresentedRemoteFileKind::Symlink,
            "other" => PresentedRemoteFileKind::Other,
            _ => return Err(remote_response_error()),
        };
        let name = validate_remote_name(utf8_field(fields[index + 1])?)?;
        let size = parse_optional_u64(fields[index + 2])?;
        let modified_at_epoch_seconds = parse_optional_i64(fields[index + 3])?;
        let permissions = optional_safe_field(fields[index + 4], 64)?;
        entries.push(PresentedRemoteFileEntry {
            path: join_remote_path(&path, &name),
            name,
            kind,
            size: (kind != PresentedRemoteFileKind::Directory)
                .then_some(size)
                .flatten(),
            modified_at_epoch_seconds,
            permissions,
        });
        index += 5;
    }
    if index != fields.len() {
        return Err(remote_response_error());
    }
    entries.sort_by(|left, right| {
        let left_directory = left.kind == PresentedRemoteFileKind::Directory;
        let right_directory = right.kind == PresentedRemoteFileKind::Directory;
        right_directory
            .cmp(&left_directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(PresentedRemoteDirectoryListing {
        parent_path: parent_remote_path(&path),
        path,
        entries,
        truncated,
    })
}

fn listing_script(path: &str) -> String {
    format!(
        "set -e; candidate={}; case \"$candidate\" in '~') candidate=\"$HOME\" ;; '~/'*) candidate=\"$HOME/${{candidate#~/}}\" ;; esac; cd -- \"$candidate\"; {}",
        shell_quote(path),
        listing_body()
    )
}

fn mutation_and_listing_script(parent_path: &str, mutation: &str) -> String {
    format!(
        "set -e; parent={}; case \"$parent\" in '~') parent=\"$HOME\" ;; '~/'*) parent=\"$HOME/${{parent#~/}}\" ;; esac; cd -- \"$parent\"; {mutation}; printf '\\0{}\\0'; set +e; {}",
        shell_quote(parent_path),
        COMMITTED_MARKER,
        listing_body()
    )
}

fn listing_body() -> String {
    format!(
        "resolved=$(pwd -P) || exit 66; printf '\\0{}\\0%s\\0' \"$resolved\"; count=0; for item in ./* ./.[!.]* ./..?*; do [ -e \"$item\" ] || [ -L \"$item\" ] || continue; count=$((count + 1)); if [ \"$count\" -gt {} ]; then printf '{}\\0'; break; fi; if [ -L \"$item\" ]; then kind=symlink; elif [ -d \"$item\" ]; then kind=directory; elif [ -f \"$item\" ]; then kind=file; else kind=other; fi; name=${{item#./}}; metadata=$(stat -c '%s %Y %A' -- \"$item\" 2>/dev/null || stat -f '%z %m %Sp' \"$item\" 2>/dev/null || true); size=${{metadata%% *}}; remainder=${{metadata#* }}; modified=${{remainder%% *}}; permissions=${{remainder#* }}; printf '%s\\0%s\\0%s\\0%s\\0%s\\0' \"$kind\" \"$name\" \"$size\" \"$modified\" \"$permissions\"; done",
        LISTING_MARKER, MAXIMUM_DIRECTORY_ENTRIES, TRUNCATED_MARKER
    )
}

fn upload_script(name: &str, expected_bytes: u64, overwrite: bool) -> String {
    let name = shell_quote(name);
    let commit = if overwrite {
        "if [ -e \"$target\" ] && [ ! -L \"$target\" ]; then mode=$(stat -c '%a' -- \"$target\" 2>/dev/null || stat -f '%Lp' \"$target\" 2>/dev/null || true); [ -z \"$mode\" ] || chmod \"$mode\" \"$tmp\"; fi; mv -f -- \"$tmp\" \"$target\""
    } else {
        "ln -- \"$tmp\" \"$target\" 2>/dev/null || exit 73; rm -f -- \"$tmp\""
    };
    format!(
        "target={name}; if [ -L \"$target\" ]; then exit 73; fi; tmp=$(mktemp './.chatos-upload.XXXXXX'); cleanup() {{ rm -f -- \"$tmp\"; }}; trap cleanup EXIT HUP INT TERM; cat > \"$tmp\"; actual=$(wc -c < \"$tmp\" | tr -d '[:space:]'); [ \"$actual\" = '{expected_bytes}' ] || exit 74; {commit}; trap - EXIT HUP INT TERM"
    )
}

fn download_script(path: &str) -> String {
    format!(
        "set -e; source={}; [ -f \"$source\" ] && [ ! -L \"$source\" ] || exit 66; size=$(stat -c '%s' -- \"$source\" 2>/dev/null || stat -f '%z' \"$source\" 2>/dev/null || exit 66); case \"$size\" in ''|*[!0-9]*) exit 66 ;; esac; [ \"$size\" -le {} ] || exit 75; printf '\\0{}\\0%s\\0' \"$size\" >&2; cat -- \"$source\"",
        shell_quote(path),
        MAXIMUM_TRANSFER_BYTES,
        DOWNLOAD_SIZE_MARKER
    )
}

fn finish_download(
    outcome: &mut ProcessOutcome,
    destination: &Path,
    overwrite: bool,
) -> Result<u64, String> {
    if outcome.timed_out {
        return Err("远端文件下载超时，未修改本机目标文件。".into());
    }
    if outcome.output_truncated || outcome.stdout_bytes > MAXIMUM_TRANSFER_BYTES {
        return Err(format!(
            "远端文件超过 {} GiB 下载上限，未修改本机目标文件。",
            MAXIMUM_TRANSFER_BYTES / 1_024 / 1_024 / 1_024
        ));
    }
    if outcome.exit_code != 0 {
        return Err(remote_file_failure(&outcome.stderr));
    }
    let expected_size = download_size_from_diagnostics(&outcome.stderr)?;
    if outcome.stdout_bytes != expected_size {
        return Err("远端文件在下载期间发生变化，未修改本机目标文件。".into());
    }
    let temporary = outcome
        .take_output_file()
        .ok_or_else(|| "远端下载没有生成本机临时文件。".to_owned())?;
    let result = revalidate_download_path(destination, overwrite)
        .and_then(|()| persist_download(&temporary, destination, overwrite));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|()| outcome.stdout_bytes)
}

fn download_size_from_diagnostics(stderr: &[u8]) -> Result<u64, String> {
    let fields = nul_fields(stderr);
    let marker = fields
        .iter()
        .position(|field| *field == DOWNLOAD_SIZE_MARKER.as_bytes())
        .ok_or_else(|| "远端下载没有返回有效的文件大小。".to_owned())?;
    let size = fields
        .get(marker + 1)
        .ok_or_else(|| "远端下载没有返回有效的文件大小。".to_owned())?;
    parse_optional_u64(size)?
        .filter(|size| *size <= MAXIMUM_TRANSFER_BYTES)
        .ok_or_else(|| "远端下载文件大小无效。".to_owned())
}

fn prepare_upload_file(path: &Path) -> Result<(PathBuf, String, u64), String> {
    let original = fs::symlink_metadata(path).map_err(|_| local_selection_error())?;
    validate_regular_local_file(&original)?;
    let canonical = fs::canonicalize(path).map_err(|_| local_selection_error())?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|_| local_selection_error())?;
    validate_regular_local_file(&metadata)?;
    if metadata.len() > MAXIMUM_TRANSFER_BYTES {
        return Err(format!(
            "单个上传文件不能超过 {} GiB。",
            MAXIMUM_TRANSFER_BYTES / 1_024 / 1_024 / 1_024
        ));
    }
    let file_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(local_selection_error)
        .and_then(validate_local_file_name)?;
    Ok((canonical, file_name, metadata.len()))
}

fn revalidate_upload_file(selection: &PendingLocalSelection) -> Result<(PathBuf, u64), String> {
    let expected_size = selection.size.ok_or_else(selection_expired)?;
    let (canonical, file_name, size) = prepare_upload_file(&selection.path)?;
    if canonical != selection.path || file_name != selection.file_name || size != expected_size {
        return Err("所选本机文件已发生变化，请重新选择。".into());
    }
    Ok((canonical, size))
}

fn prepare_download_destination(path: &Path) -> Result<(PathBuf, String), String> {
    let parent = path.parent().ok_or_else(local_selection_error)?;
    let canonical_parent = fs::canonicalize(parent).map_err(|_| local_selection_error())?;
    let parent_metadata =
        fs::symlink_metadata(&canonical_parent).map_err(|_| local_selection_error())?;
    if !parent_metadata.is_dir()
        || parent_metadata.file_type().is_symlink()
        || is_reparse_point(&parent_metadata)
    {
        return Err(local_selection_error());
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(local_selection_error)
        .and_then(validate_local_file_name)?;
    let destination = canonical_parent.join(&file_name);
    if let Ok(metadata) = fs::symlink_metadata(&destination) {
        validate_regular_local_file(&metadata)?;
    }
    Ok((destination, file_name))
}

fn revalidate_download_destination(
    selection: &PendingLocalSelection,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let (destination, file_name) = prepare_download_destination(&selection.path)?;
    if destination != selection.path || file_name != selection.file_name {
        return Err("本机下载位置已发生变化，请重新选择。".into());
    }
    revalidate_download_path(&destination, overwrite)?;
    Ok(destination)
}

fn revalidate_download_path(destination: &Path, overwrite: bool) -> Result<(), String> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) => {
            validate_regular_local_file(&metadata)?;
            if !overwrite {
                return Err("本机目标文件已存在，请确认覆盖后重试。".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(local_selection_error()),
    }
    Ok(())
}

fn validate_regular_local_file(metadata: &fs::Metadata) -> Result<(), String> {
    if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(metadata) {
        Err(local_selection_error())
    } else {
        Ok(())
    }
}

fn download_temporary_path(
    destination: &Path,
    selection_id: &str,
    operation_id: &str,
) -> Result<PathBuf, String> {
    validate_selection_id(selection_id)?;
    validate_operation_id(operation_id)?;
    let parent = destination.parent().ok_or_else(local_selection_error)?;
    let mut digest = Sha256::new();
    digest.update(selection_id.as_bytes());
    digest.update([0]);
    digest.update(operation_id.as_bytes());
    let suffix = format!("{:x}", digest.finalize());
    Ok(parent.join(format!(".chatos-download-{}.tmp", &suffix[..24])))
}

fn persist_download(source: &Path, destination: &Path, overwrite: bool) -> Result<(), String> {
    if overwrite {
        replace_file(source, destination).map_err(|_| "无法保存下载文件。".to_owned())
    } else {
        fs::hard_link(source, destination).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "本机目标文件已存在，请确认覆盖后重试。".to_owned()
            } else {
                "无法保存下载文件。".to_owned()
            }
        })?;
        let _ = fs::remove_file(source);
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn validate_operation_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Err("远端文件操作标识无效。".into())
    } else {
        Ok(())
    }
}

fn validate_presentation_id(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        Err("远端文件窗口标识无效。".into())
    } else {
        Ok(())
    }
}

fn validate_selection_id(value: &str) -> Result<(), String> {
    if value.starts_with("remote-file-selection-")
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(selection_expired())
    }
}

fn validate_local_file_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAXIMUM_REMOTE_NAME_BYTES
        || matches!(value, "." | "..")
        || value.contains(['/', '\\', '\0'])
        || value.chars().any(char::is_control)
    {
        Err("本机文件名无效。".into())
    } else {
        Ok(value.to_owned())
    }
}

fn validate_remote_path(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAXIMUM_REMOTE_PATH_BYTES
        || value.chars().any(char::is_control)
    {
        Err("远端路径无效。".into())
    } else {
        Ok(value.to_owned())
    }
}

fn validate_resolved_remote_path(value: &str) -> Result<String, String> {
    let value = validate_remote_path(value)?;
    if !value.starts_with('/') || (value.len() > 1 && value.ends_with('/')) {
        Err(remote_response_error())
    } else {
        Ok(value)
    }
}

fn validate_mutable_remote_path(value: &str) -> Result<String, String> {
    let value = validate_remote_path(value)?;
    if matches!(value.as_str(), "/" | "." | ".." | "~") || value.ends_with('/') {
        Err("不能修改远端根目录。".into())
    } else {
        Ok(value)
    }
}

fn validate_remote_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAXIMUM_REMOTE_NAME_BYTES
        || matches!(value, "." | "..")
        || value.contains(['/', '\0'])
        || value.chars().any(char::is_control)
    {
        Err("远端文件名无效。".into())
    } else {
        Ok(value.to_owned())
    }
}

fn split_remote_path(path: &str) -> Result<(String, String), String> {
    let (parent, name) = match path.rsplit_once('/') {
        Some(("", name)) => ("/".to_owned(), name),
        Some((parent, name)) => (parent.to_owned(), name),
        None => (".".to_owned(), path),
    };
    Ok((validate_remote_path(&parent)?, validate_remote_name(name)?))
}

fn parent_remote_path(path: &str) -> Option<String> {
    if path == "/" {
        return None;
    }
    path.rsplit_once('/').map_or_else(
        || Some(".".into()),
        |(parent, _)| Some(if parent.is_empty() { "/" } else { parent }.into()),
    )
}

fn join_remote_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn nul_fields(bytes: &[u8]) -> Vec<&[u8]> {
    let mut fields = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    if fields.last().is_some_and(|field| field.is_empty()) {
        fields.pop();
    }
    fields
}

fn has_nul_field(bytes: &[u8], expected: &[u8]) -> bool {
    nul_fields(bytes).contains(&expected)
}

fn utf8_field(bytes: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(bytes).map_err(|_| remote_response_error())
}

fn parse_optional_u64(bytes: &[u8]) -> Result<Option<u64>, String> {
    if bytes.is_empty() {
        Ok(None)
    } else {
        utf8_field(bytes)?
            .parse()
            .map(Some)
            .map_err(|_| remote_response_error())
    }
}

fn parse_optional_i64(bytes: &[u8]) -> Result<Option<i64>, String> {
    if bytes.is_empty() {
        Ok(None)
    } else {
        utf8_field(bytes)?
            .parse()
            .map(Some)
            .map_err(|_| remote_response_error())
    }
}

fn optional_safe_field(bytes: &[u8], maximum: usize) -> Result<Option<String>, String> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let value = utf8_field(bytes)?;
    if value.len() > maximum || value.chars().any(char::is_control) {
        return Err(remote_response_error());
    }
    Ok(Some(value.to_owned()))
}

fn remote_file_failure(stderr: &[u8]) -> String {
    let lower = String::from_utf8_lossy(stderr).to_lowercase();
    if lower.contains("no such file") || lower.contains("not found") {
        "远端文件或目录不存在。".into()
    } else if lower.contains("file exists") {
        "远端已存在同名文件或目录。".into()
    } else if lower.contains("permission denied (") || lower.contains("permission denied,") {
        user_facing_failure(stderr)
    } else if lower.contains("permission denied") {
        "没有权限执行远端文件操作。".into()
    } else if lower.contains("is a directory") || lower.contains("not a directory") {
        "远端文件类型与当前操作不匹配。".into()
    } else {
        user_facing_failure(stderr)
    }
}

fn remote_response_error() -> String {
    "远端目录返回了无法安全解析的数据。".into()
}

fn local_selection_error() -> String {
    "无法安全使用所选本机文件。".into()
}

fn selection_expired() -> String {
    "本机文件选择已失效，请重新选择。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    fn listing_bytes() -> Vec<u8> {
        [
            LISTING_MARKER,
            "/srv/app",
            "file",
            "app.log",
            "42",
            "10",
            "-rw-r--r--",
            "directory",
            "logs",
            "4096",
            "20",
            "drwxr-xr-x",
        ]
        .join("\0")
        .into_bytes()
    }

    fn outcome(stdout: Vec<u8>, exit_code: i32) -> ProcessOutcome {
        ProcessOutcome {
            exit_code,
            stdout_bytes: stdout.len() as u64,
            stdout,
            stderr: Vec::new(),
            prompts: String::new(),
            timed_out: false,
            output_truncated: false,
            output_file: None,
        }
    }

    #[test]
    fn listing_projection_is_bounded_sorted_and_contains_no_local_paths() {
        let listing = parse_listing(&listing_bytes()).unwrap();
        assert_eq!(listing.path, "/srv/app");
        assert_eq!(listing.parent_path.as_deref(), Some("/srv"));
        assert_eq!(listing.entries[0].name, "logs");
        assert_eq!(listing.entries[0].kind, PresentedRemoteFileKind::Directory);
        assert_eq!(listing.entries[0].size, None);
        assert_eq!(listing.entries[1].path, "/srv/app/app.log");
        assert_eq!(listing.entries[1].size, Some(42));
    }

    #[test]
    fn listing_rejects_control_names_invalid_kinds_and_malformed_tuples() {
        let invalid_name = [LISTING_MARKER, "/srv", "file", "bad\nname", "1", "1", "-"].join("\0");
        assert!(parse_listing(invalid_name.as_bytes()).is_err());
        let invalid_kind = [LISTING_MARKER, "/srv", "socket", "name", "1", "1", "-"].join("\0");
        assert!(parse_listing(invalid_kind.as_bytes()).is_err());
        assert!(parse_listing(format!("{LISTING_MARKER}\0/srv\0file").as_bytes()).is_err());
    }

    #[test]
    fn listing_parser_ignores_bounded_login_banner_before_its_nul_marker() {
        let mut output = b"Authorized access only".to_vec();
        output.push(0);
        output.extend(listing_bytes());

        let listing = parse_listing(&output).unwrap();

        assert_eq!(listing.path, "/srv/app");
        assert_eq!(listing.entries.len(), 2);
    }

    #[test]
    fn mutation_reports_committed_without_refresh_instead_of_false_failure() {
        let stdout = format!("{COMMITTED_MARKER}\0").into_bytes();
        let result = finish_mutation(outcome(stdout, 86)).unwrap();
        assert!(result.committed);
        assert!(result.refreshed_listing.is_none());
        assert!(result.message.is_some());
    }

    #[test]
    fn mutation_returns_refreshed_listing_from_the_same_ssh_operation() {
        let mut stdout = format!("{COMMITTED_MARKER}\0").into_bytes();
        stdout.extend(listing_bytes());
        let result = finish_mutation(outcome(stdout, 0)).unwrap();
        assert!(result.committed);
        assert_eq!(result.refreshed_listing.unwrap().path, "/srv/app");
        assert!(result.message.is_none());
    }

    #[test]
    fn remote_paths_and_names_reject_root_mutation_and_control_data() {
        assert!(validate_mutable_remote_path("/").is_err());
        assert!(validate_mutable_remote_path(".").is_err());
        assert!(validate_remote_name("../escape").is_err());
        assert!(validate_remote_name("bad\nname").is_err());
        assert_eq!(
            split_remote_path("/srv/app.log").unwrap(),
            ("/srv".into(), "app.log".into())
        );
    }

    #[test]
    fn shell_scripts_quote_every_caller_controlled_value() {
        let path = "/srv/it's here";
        let listing = listing_script(path);
        assert!(listing.contains("'/srv/it'\\''s here'"));
        let mutation = mutation_and_listing_script(path, "mkdir -- 'child'");
        assert!(mutation.contains("'/srv/it'\\''s here'"));
        assert!(mutation.contains(COMMITTED_MARKER));
        assert!(mutation.contains(LISTING_MARKER));
    }

    #[test]
    fn errors_are_classified_without_exposing_remote_stderr() {
        let message = remote_file_failure(b"Permission denied: secret /home/alice/private");
        assert_eq!(message, "没有权限执行远端文件操作。");
        assert!(!message.contains("secret"));
        assert!(!message.contains("/home"));
    }

    #[test]
    fn local_selection_is_owner_presentation_and_kind_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.txt");
        fs::write(&path, b"report").unwrap();
        let store = RemoteFileSelectionStore::default();
        let selected = store
            .stage(
                "owner-a",
                "sheet-a",
                LocalSelectionKind::Upload,
                path.clone(),
                "report.txt".into(),
                Some(6),
            )
            .unwrap();

        assert!(store
            .resolve(
                "owner-b",
                "sheet-a",
                &selected.selection_id,
                LocalSelectionKind::Upload,
            )
            .is_err());
        assert!(store
            .resolve(
                "owner-a",
                "sheet-b",
                &selected.selection_id,
                LocalSelectionKind::Upload,
            )
            .is_err());
        assert!(store
            .resolve(
                "owner-a",
                "sheet-a",
                &selected.selection_id,
                LocalSelectionKind::Download,
            )
            .is_err());
        assert_eq!(
            store
                .resolve(
                    "owner-a",
                    "sheet-a",
                    &selected.selection_id,
                    LocalSelectionKind::Upload,
                )
                .unwrap()
                .path,
            path
        );
        store.reset();
        assert!(store
            .resolve(
                "owner-a",
                "sheet-a",
                &selected.selection_id,
                LocalSelectionKind::Upload,
            )
            .is_err());
    }

    #[test]
    fn upload_script_uses_a_remote_temp_and_exact_size_before_commit() {
        let script = upload_script("it's-report.txt", 42, false);
        assert!(script.contains("target='it'\\''s-report.txt'"));
        assert!(script.contains(".chatos-upload.XXXXXX"));
        assert!(script.contains("[ \"$actual\" = '42' ]"));
        assert!(script.contains("ln -- \"$tmp\" \"$target\""));
        assert!(!script.contains("/Users/"));
        assert!(!script.contains("C:\\"));
    }

    #[test]
    fn download_commit_is_atomic_and_never_overwrites_without_confirmation() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download.txt");
        let temporary = directory.path().join("download.tmp");
        fs::write(&temporary, b"new content").unwrap();
        let created = Arc::new(AtomicBool::new(true));
        let mut outcome = ProcessOutcome {
            exit_code: 0,
            stdout: Vec::new(),
            stdout_bytes: 11,
            stderr: format!("ssh diagnostic\0{DOWNLOAD_SIZE_MARKER}\011\0").into_bytes(),
            prompts: String::new(),
            timed_out: false,
            output_truncated: false,
            output_file: Some(super::super::ssh::ProcessOutputFile::new(
                temporary.clone(),
                created.clone(),
            )),
        };

        assert_eq!(
            finish_download(&mut outcome, &destination, false).unwrap(),
            11
        );
        assert_eq!(fs::read(&destination).unwrap(), b"new content");
        assert!(!temporary.exists());
        assert!(created.load(Ordering::SeqCst));

        let second = directory.path().join("second.tmp");
        fs::write(&second, b"replacement").unwrap();
        let mut second_outcome = ProcessOutcome {
            exit_code: 0,
            stdout: Vec::new(),
            stdout_bytes: 11,
            stderr: format!("\0{DOWNLOAD_SIZE_MARKER}\011\0").into_bytes(),
            prompts: String::new(),
            timed_out: false,
            output_truncated: false,
            output_file: Some(super::super::ssh::ProcessOutputFile::new(
                second.clone(),
                Arc::new(AtomicBool::new(true)),
            )),
        };
        assert!(finish_download(&mut second_outcome, &destination, false).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"new content");
        assert!(!second.exists());
    }

    #[cfg(unix)]
    #[test]
    fn upload_selection_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("source.txt");
        let link = directory.path().join("link.txt");
        fs::write(&file, b"content").unwrap();
        symlink(&file, &link).unwrap();

        assert!(prepare_upload_file(&link).is_err());
    }
}
