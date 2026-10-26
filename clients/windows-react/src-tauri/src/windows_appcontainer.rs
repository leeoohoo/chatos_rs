use std::{
    collections::{HashMap, HashSet},
    ffi::{c_void, OsStr, OsString},
    fs::{File, OpenOptions},
    io::{Read, Write},
    mem::{size_of, zeroed},
    net::TcpListener,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle, RawHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
    thread,
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, LocalFree, SetHandleInformation, ERROR_BROKEN_PIPE, HANDLE,
        HANDLE_FLAG_INHERIT, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::{
        Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
            ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
            GetSecurityInfo, SetEntriesInAclW, SetSecurityInfo, EXPLICIT_ACCESS_W, GRANT_ACCESS,
            REVOKE_ACCESS, SDDL_REVISION_1, SE_FILE_OBJECT, TRUSTEE_IS_SID,
            TRUSTEE_IS_WELL_KNOWN_GROUP,
        },
        CreateWellKnownSid, FreeSid, GetSecurityDescriptorDacl, GetTokenInformation,
        Isolation::{CreateAppContainerProfile, DeleteAppContainerProfile},
        SetFileSecurityW, TokenCapabilities, WinCapabilityInternetClientSid,
        DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
        SECURITY_CAPABILITIES, SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES,
        SUB_CONTAINERS_AND_OBJECTS_INHERIT, TOKEN_GROUPS, TOKEN_QUERY,
    },
    Storage::FileSystem::{
        GetFileInformationByHandle, GetFinalPathNameByHandleW, ReadFile,
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC,
    },
    System::Pipes::CreatePipe,
    System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
        InitializeProcThreadAttributeList, OpenProcessToken, ResumeThread, TerminateProcess,
        UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST,
        PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    },
};

use crate::{
    app_data_transaction::create_owned_file, terminal_runtime::process::ProcessTermination,
};

#[cfg(test)]
use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SetNamedSecurityInfoW};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppContainerProbe {
    pub filesystem_denied: bool,
    pub network_denied: bool,
    pub workspace_lease_verified: bool,
    pub workspace_recovery_verified: bool,
    pub workspace_permission_tiers_verified: bool,
    pub nonempty_workspace_tree_verified: bool,
    pub network_capability_policy_verified: bool,
    pub runtime_launcher_verified: bool,
}

static PROBE: OnceLock<Result<AppContainerProbe, String>> = OnceLock::new();
static FIXED_PROFILE_IN_USE: Mutex<bool> = Mutex::new(false);
static FIXED_PROFILE_AVAILABLE: Condvar = Condvar::new();
const MAXIMUM_LEASE_TREE_ENTRIES: usize = 100_000;
const MAXIMUM_LEASE_JOURNAL_BYTES: usize = 64 * 1_024 * 1_024;
const WORKSPACE_JOURNAL_TEMPORARY_PREFIX: &str = ".appcontainer-workspace-";
const PROFILE_JOURNAL_TEMPORARY_PREFIX: &str = ".appcontainer-profile-";

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetworkAccess {
    Disabled,
    Controlled,
}

impl NetworkAccess {
    fn from_policy(value: &str) -> Result<Self, String> {
        match value {
            "disabled" => Ok(Self::Disabled),
            "controlled" => Ok(Self::Controlled),
            _ => Err("The AppContainer network access policy is unsupported.".to_owned()),
        }
    }
}

struct CapabilitySet {
    _sid_storage: Vec<u8>,
    attributes: Vec<SID_AND_ATTRIBUTES>,
}

impl CapabilitySet {
    fn for_network_access(access: NetworkAccess) -> Result<Self, String> {
        match access {
            NetworkAccess::Disabled => Ok(Self {
                _sid_storage: Vec::new(),
                attributes: Vec::new(),
            }),
            NetworkAccess::Controlled => {
                let mut sid_storage = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
                let mut sid_size = sid_storage.len() as u32;
                let created = unsafe {
                    CreateWellKnownSid(
                        WinCapabilityInternetClientSid,
                        null_mut(),
                        sid_storage.as_mut_ptr().cast(),
                        &mut sid_size,
                    )
                };
                if created == 0 {
                    return Err(last_error(
                        "Creating the Windows internetClient capability SID failed",
                    ));
                }
                sid_storage.truncate(sid_size as usize);
                let attributes = vec![SID_AND_ATTRIBUTES {
                    Sid: sid_storage.as_mut_ptr().cast(),
                    Attributes: 0x4,
                }];
                Ok(Self {
                    _sid_storage: sid_storage,
                    attributes,
                })
            }
        }
    }

    fn pointer(&mut self) -> *mut SID_AND_ATTRIBUTES {
        if self.attributes.is_empty() {
            null_mut()
        } else {
            self.attributes.as_mut_ptr()
        }
    }

    fn expected_sid(&self) -> Option<PSID> {
        self.attributes.first().map(|entry| entry.Sid)
    }
}

struct AppContainerProcessResult {
    exit_code: u32,
    capability_policy_verified: bool,
}

struct AppContainerCapturedProcessResult {
    exit_code: u32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    capability_policy_verified: bool,
}

struct AppContainerInteractiveProcess {
    termination: ProcessTermination,
    process: ProcessHandles,
    stdin: File,
    stdout: File,
    stderr: File,
}

#[derive(Debug)]
pub(crate) struct SandboxedCommandOutput {
    pub(crate) exit_code: i32,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

pub(crate) struct SandboxedChild {
    termination: ProcessTermination,
    process: ProcessHandles,
    stdin: Option<File>,
    stdout: Option<File>,
    stderr: Option<File>,
    leases: Vec<DirectoryLease>,
    _profile: AppContainerProfile,
}

impl SandboxedChild {
    pub(crate) fn take_stdin(&mut self) -> Option<File> {
        self.stdin.take()
    }

    pub(crate) fn take_stdout(&mut self) -> Option<File> {
        self.stdout.take()
    }

    pub(crate) fn take_stderr(&mut self) -> Option<File> {
        self.stderr.take()
    }

    pub(crate) fn terminate(&self) -> Result<(), String> {
        self.termination.terminate(true).map_err(|error| {
            format!("Terminating the interactive AppContainer process failed: {error}")
        })
    }

    pub(crate) fn wait(mut self, timeout: Duration) -> Result<i32, String> {
        drop(self.stdin.take());
        let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        let process_result = match unsafe { WaitForSingleObject(self.process.process, timeout_ms) }
        {
            WAIT_OBJECT_0 => {
                let _ = self.termination.terminate(true);
                let mut exit_code = 0;
                if unsafe { GetExitCodeProcess(self.process.process, &mut exit_code) } == 0 {
                    Err(last_error(
                        "Reading the interactive AppContainer process exit code failed",
                    ))
                } else {
                    Ok(i32::try_from(exit_code).unwrap_or(-1))
                }
            }
            WAIT_TIMEOUT => {
                let _ = self.termination.terminate(true);
                unsafe { WaitForSingleObject(self.process.process, 5_000) };
                Err("The interactive AppContainer process timed out.".to_owned())
            }
            _ => {
                let error = last_error("Waiting for the interactive AppContainer process failed");
                let _ = self.termination.terminate(true);
                unsafe { WaitForSingleObject(self.process.process, 5_000) };
                Err(error)
            }
        };
        let restored = restore_directory_leases(std::mem::take(&mut self.leases));
        match (process_result, restored) {
            (Ok(exit_code), Ok(())) => Ok(exit_code),
            (Err(process), Ok(())) => Err(process),
            (Ok(_), Err(restore)) => Err(format!(
                "Restoring the interactive AppContainer workspace failed: {restore}"
            )),
            (Err(process), Err(restore)) => Err(format!(
                "{process} Restoring the interactive AppContainer workspace also failed: {restore}"
            )),
        }
    }
}

pub(crate) struct WorkspaceLeaseManager {
    journal_root: PathBuf,
    profile_journal_root: PathBuf,
    active_workspaces: Arc<Mutex<HashSet<PathBuf>>>,
}

impl WorkspaceLeaseManager {
    pub(crate) fn open(app_data_directory: &Path) -> Result<Self, String> {
        let journal_root = app_data_directory
            .join("local-agent")
            .join("appcontainer-workspace-leases");
        std::fs::create_dir_all(&journal_root).map_err(|error| {
            format!("Creating the AppContainer workspace lease directory failed: {error}")
        })?;
        let journal_root = std::fs::canonicalize(&journal_root).map_err(|error| {
            format!("Resolving the AppContainer workspace lease directory failed: {error}")
        })?;
        let profile_journal_root = app_data_directory
            .join("local-agent")
            .join("appcontainer-profile-leases");
        std::fs::create_dir_all(&profile_journal_root).map_err(|error| {
            format!("Creating the AppContainer profile lease directory failed: {error}")
        })?;
        let profile_journal_root =
            std::fs::canonicalize(&profile_journal_root).map_err(|error| {
                format!("Resolving the AppContainer profile lease directory failed: {error}")
            })?;
        let manager = Self {
            journal_root,
            profile_journal_root,
            active_workspaces: Arc::new(Mutex::new(HashSet::new())),
        };
        manager.recover_all()?;
        manager.recover_profiles()?;
        Ok(manager)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_workspace_command(
        &self,
        workspace: &Path,
        executable: &Path,
        arguments: &[String],
        access: WorkspaceAccess,
        network_policy: &str,
        environment: &[(OsString, OsString)],
        timeout: Duration,
        maximum_output_bytes: usize,
    ) -> Result<SandboxedCommandOutput, String> {
        if maximum_output_bytes == 0 || maximum_output_bytes > 64 * 1_024 * 1_024 {
            return Err("The AppContainer process output limit is invalid.".to_owned());
        }
        let profile = AppContainerProfile::create_journaled(&self.profile_journal_root)?;
        let lease = self.grant(workspace, profile.sid, access)?;
        let command_line = build_windows_command_line(executable.as_os_str(), arguments)?;
        let mut environment = build_environment_block(environment)?;
        let process = profile.run_captured(
            executable,
            &command_line,
            workspace,
            timeout,
            NetworkAccess::from_policy(network_policy)?,
            Some(&mut environment),
            maximum_output_bytes,
        );
        let restored = lease.restore();
        match (process, restored) {
            (Ok(process), Ok(())) if process.capability_policy_verified => {
                Ok(SandboxedCommandOutput {
                    exit_code: i32::try_from(process.exit_code).unwrap_or(-1),
                    stdout: process.stdout,
                    stderr: process.stderr,
                })
            }
            (Ok(_), Ok(())) => Err(
                "The AppContainer runtime process capability policy was not verified.".to_owned(),
            ),
            (Err(process), Ok(())) => Err(process),
            (Ok(_), Err(restore)) => Err(format!(
                "Restoring the AppContainer runtime workspace failed: {restore}"
            )),
            (Err(process), Err(restore)) => Err(format!(
                "{process} Restoring the AppContainer runtime workspace also failed: {restore}"
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_workspace_command(
        &self,
        workspace: &Path,
        executable: &Path,
        arguments: &[String],
        access: WorkspaceAccess,
        network_policy: &str,
        environment: &[(OsString, OsString)],
    ) -> Result<SandboxedChild, String> {
        self.spawn_workspace_command_with_leases(
            workspace,
            executable,
            arguments,
            &[(workspace.to_owned(), access)],
            network_policy,
            environment,
        )
    }

    pub(crate) fn spawn_workspace_command_with_leases(
        &self,
        current_directory: &Path,
        executable: &Path,
        arguments: &[String],
        workspace_leases: &[(PathBuf, WorkspaceAccess)],
        network_policy: &str,
        environment: &[(OsString, OsString)],
    ) -> Result<SandboxedChild, String> {
        if workspace_leases.is_empty() || workspace_leases.len() > 16 {
            return Err("The interactive AppContainer workspace lease set is invalid.".to_owned());
        }
        let mut canonical_leases: Vec<(PathBuf, WorkspaceAccess)> =
            Vec::with_capacity(workspace_leases.len());
        for (workspace, access) in workspace_leases {
            let workspace = std::fs::canonicalize(workspace).map_err(|error| {
                format!("Resolving an interactive AppContainer workspace failed: {error}")
            })?;
            if !workspace.is_dir()
                || canonical_leases.iter().any(|(existing, _)| {
                    workspace.starts_with(existing) || existing.starts_with(&workspace)
                })
            {
                return Err(
                    "The interactive AppContainer workspace leases overlap or are invalid."
                        .to_owned(),
                );
            }
            canonical_leases.push((workspace, *access));
        }
        let current_directory = std::fs::canonicalize(current_directory).map_err(|error| {
            format!("Resolving the interactive AppContainer working directory failed: {error}")
        })?;
        if !current_directory.is_dir()
            || !canonical_leases
                .iter()
                .any(|(workspace, _)| current_directory.starts_with(workspace))
        {
            return Err(
                "The interactive AppContainer working directory is outside its leases.".to_owned(),
            );
        }
        let command_line = build_windows_command_line(executable.as_os_str(), arguments)?;
        let mut environment = build_environment_block(environment)?;
        let network_access = NetworkAccess::from_policy(network_policy)?;
        let profile = AppContainerProfile::create_journaled(&self.profile_journal_root)?;
        let mut leases = Vec::with_capacity(canonical_leases.len());
        for (workspace, access) in &canonical_leases {
            leases.push(self.grant(workspace, profile.sid, *access)?);
        }
        let process = profile.spawn_interactive(
            executable,
            &command_line,
            &current_directory,
            network_access,
            Some(&mut environment),
        );
        match process {
            Ok(process) => Ok(SandboxedChild {
                termination: process.termination,
                process: process.process,
                stdin: Some(process.stdin),
                stdout: Some(process.stdout),
                stderr: Some(process.stderr),
                leases,
                _profile: profile,
            }),
            Err(process) => match restore_directory_leases(leases) {
                Ok(()) => Err(process),
                Err(restore) => Err(format!(
                    "{process} Restoring the interactive AppContainer workspace also failed: {restore}"
                )),
            },
        }
    }

    fn grant(
        &self,
        workspace: &Path,
        sid: PSID,
        access: WorkspaceAccess,
    ) -> Result<DirectoryLease, String> {
        let workspace = std::fs::canonicalize(workspace)
            .map_err(|error| format!("Resolving the AppContainer workspace failed: {error}"))?;
        if !workspace.is_dir() {
            return Err("The AppContainer workspace is not a directory.".to_owned());
        }
        {
            let mut active = self.active_workspaces.lock().map_err(|_| {
                "The AppContainer workspace lease registry is unavailable.".to_owned()
            })?;
            if !active.insert(workspace.clone()) {
                return Err("The AppContainer workspace already has an active lease.".to_owned());
            }
        }
        let journal_path = self
            .journal_root
            .join(format!("{}.json", uuid::Uuid::new_v4()));
        match DirectoryLease::grant(
            &workspace,
            sid,
            access,
            &journal_path,
            self.active_workspaces.clone(),
        ) {
            Ok(lease) => Ok(lease),
            Err(error) => {
                if let Ok(mut active) = self.active_workspaces.lock() {
                    active.remove(&workspace);
                }
                Err(error)
            }
        }
    }

    fn recover_all(&self) -> Result<usize, String> {
        let mut entries = std::fs::read_dir(&self.journal_root)
            .map_err(|error| format!("Reading AppContainer workspace leases failed: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Reading AppContainer workspace leases failed: {error}"))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        if entries.len() > 1_024 {
            return Err(
                "Too many AppContainer workspace lease records require recovery.".to_owned(),
            );
        }
        let mut recovered = 0;
        for entry in entries {
            let file_type = entry.file_type().map_err(|error| {
                format!("Inspecting an AppContainer workspace lease failed: {error}")
            })?;
            if !file_type.is_file() {
                return Err("The AppContainer workspace lease directory is invalid.".to_owned());
            }
            let path = entry.path();
            let name = entry.file_name();
            if canonical_uuid_name(&name, WORKSPACE_JOURNAL_TEMPORARY_PREFIX, ".tmp")
                || canonical_uuid_name(&name, "", ".tmp")
            {
                std::fs::remove_file(&path).map_err(|error| {
                    format!("Removing an incomplete workspace lease journal failed: {error}")
                })?;
            } else if path.extension().and_then(|value| value.to_str()) == Some("json") {
                DirectoryLease::recover(&path)?;
                recovered += 1;
            } else {
                return Err(
                    "The AppContainer workspace lease directory contains an unknown record."
                        .to_owned(),
                );
            }
        }
        Ok(recovered)
    }

    fn recover_profiles(&self) -> Result<usize, String> {
        let mut entries = std::fs::read_dir(&self.profile_journal_root)
            .map_err(|error| format!("Reading AppContainer profile leases failed: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Reading AppContainer profile leases failed: {error}"))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        if entries.len() > 1_024 {
            return Err("Too many AppContainer profile leases require recovery.".to_owned());
        }
        let mut recovered = 0;
        for entry in entries {
            let file_type = entry.file_type().map_err(|error| {
                format!("Inspecting an AppContainer profile lease failed: {error}")
            })?;
            if !file_type.is_file() {
                return Err("The AppContainer profile lease directory is invalid.".to_owned());
            }
            let path = entry.path();
            let name = entry.file_name();
            if canonical_uuid_name(&name, PROFILE_JOURNAL_TEMPORARY_PREFIX, ".tmp")
                || canonical_uuid_name(&name, "", ".tmp")
            {
                std::fs::remove_file(&path).map_err(|error| {
                    format!("Removing an incomplete AppContainer profile lease failed: {error}")
                })?;
            } else if canonical_uuid_name(&name, "", ".json") {
                recover_profile_journal(&path)?;
                recovered += 1;
            } else {
                return Err(
                    "The AppContainer profile lease directory contains an unknown record."
                        .to_owned(),
                );
            }
        }
        Ok(recovered)
    }

    #[cfg(test)]
    fn journal_count(&self) -> usize {
        std::fs::read_dir(&self.journal_root)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(usize::MAX)
    }

    #[cfg(test)]
    fn profile_journal_count(&self) -> usize {
        std::fs::read_dir(&self.profile_journal_root)
            .map(|entries| entries.count())
            .unwrap_or(usize::MAX)
    }
}

pub(crate) fn probe_isolation() -> Result<AppContainerProbe, String> {
    PROBE.get_or_init(run_probe).clone()
}

fn run_probe() -> Result<AppContainerProbe, String> {
    let directory =
        tempfile::tempdir().map_err(|error| format!("无法创建 AppContainer 探测目录：{error}"))?;
    let protected_file = directory.path().join("host-only.txt");
    std::fs::write(&protected_file, b"host-only")
        .map_err(|error| format!("无法写入 AppContainer 探测文件：{error}"))?;

    let system_root =
        std::env::var_os("SystemRoot").ok_or_else(|| "Windows SystemRoot 不可用。".to_owned())?;
    let reader = Path::new(&system_root).join("System32").join("findstr.exe");
    if !reader.is_file() {
        return Err("Windows findstr.exe 不存在，无法验证 AppContainer。".into());
    }

    let profile_name = "ChatOS.AppContainer.Probe";
    let profile = AppContainerProfile::create(profile_name)?;
    let command_line = format!(
        "\"{}\" /r /c:\"host-only\" \"{}\"",
        reader.display(),
        protected_file.display()
    );
    let disabled_network_process = profile.run_with_network(
        &reader,
        &command_line,
        Duration::from_secs(10),
        NetworkAccess::from_policy("disabled")?,
    )?;
    let controlled_network_process = profile.run_with_network(
        &reader,
        &command_line,
        Duration::from_secs(10),
        NetworkAccess::from_policy("controlled")?,
    )?;
    let network_denied = probe_network_isolation(&profile, Path::new(&system_root))?;
    let workspace = tempfile::tempdir()
        .map_err(|error| format!("无法创建 AppContainer 工作区租约探测目录：{error}"))?;
    let lease_data = tempfile::tempdir()
        .map_err(|error| format!("无法创建 AppContainer 租约 journal 探测目录：{error}"))?;
    let lease_manager = WorkspaceLeaseManager::open(lease_data.path())?;
    let lease = lease_manager.grant(workspace.path(), profile.sid, WorkspaceAccess::ReadOnly)?;
    let journal_path = lease.journal_path.clone();
    let workspace_file = workspace.path().join("workspace.txt");
    std::fs::write(&workspace_file, b"lease-readable")
        .map_err(|error| format!("无法写入 AppContainer 工作区租约探测文件：{error}"))?;
    let granted_exit = read_probe_file(&profile, &reader, &workspace_file)?;
    let read_only_write_file = workspace.path().join("read-only-write.txt");
    let read_only_write_exit =
        write_probe_file(&profile, Path::new(&system_root), &read_only_write_file)?;
    lease.restore()?;
    let restored_exit = read_probe_file(&profile, &reader, &workspace_file)?;
    let recovery_workspace = tempfile::tempdir()
        .map_err(|error| format!("无法创建 AppContainer 恢复探测目录：{error}"))?;
    let recovery_lease = lease_manager.grant(
        recovery_workspace.path(),
        profile.sid,
        WorkspaceAccess::ReadOnly,
    )?;
    let recovery_journal_path = recovery_lease.journal_path.clone();
    let recovery_file = recovery_workspace.path().join("recovery.txt");
    std::fs::write(&recovery_file, b"lease-readable")
        .map_err(|error| format!("无法写入 AppContainer 恢复探测文件：{error}"))?;
    let recovery_granted_exit = read_probe_file(&profile, &reader, &recovery_file)?;
    recovery_lease.abandon_for_recovery();
    let recovered_count = lease_manager.recover_all()?;
    let recovered_exit = read_probe_file(&profile, &reader, &recovery_file)?;
    let read_write_workspace = tempfile::tempdir()
        .map_err(|error| format!("无法创建 AppContainer 读写探测目录：{error}"))?;
    let read_write_lease = lease_manager.grant(
        read_write_workspace.path(),
        profile.sid,
        WorkspaceAccess::ReadWrite,
    )?;
    let read_write_file = read_write_workspace.path().join("read-write.txt");
    let read_write_exit = write_probe_file(&profile, Path::new(&system_root), &read_write_file)?;
    let read_write_created = std::fs::read_to_string(&read_write_file)
        .map(|value| value.trim() == "lease-written")
        .unwrap_or(false);
    read_write_lease.restore()?;
    let after_read_write_restore_read_exit = read_probe_file(&profile, &reader, &read_write_file)?;
    std::fs::remove_file(&read_write_file)
        .map_err(|error| format!("无法清理 AppContainer 读写探测文件：{error}"))?;
    let after_read_write_restore_exit =
        write_probe_file(&profile, Path::new(&system_root), &read_write_file)?;
    let nonempty_workspace = tempfile::tempdir()
        .map_err(|error| format!("无法创建 AppContainer 非空目录探测：{error}"))?;
    let existing_directory = nonempty_workspace.path().join("existing");
    std::fs::create_dir(&existing_directory)
        .map_err(|error| format!("无法创建 AppContainer 既有子目录：{error}"))?;
    let existing_file = existing_directory.join("existing.txt");
    std::fs::write(&existing_file, b"lease-readable")
        .map_err(|error| format!("无法写入 AppContainer 非空目录探测文件：{error}"))?;
    let nonempty_lease = lease_manager.grant(
        nonempty_workspace.path(),
        profile.sid,
        WorkspaceAccess::ReadOnly,
    )?;
    let nonempty_granted_exit = read_probe_file(&profile, &reader, &existing_file)?;
    nonempty_lease.restore()?;
    let nonempty_restored_exit = read_probe_file(&profile, &reader, &existing_file)?;
    let nonempty_workspace_tree_verified =
        nonempty_granted_exit == 0 && nonempty_restored_exit != 0;
    let network_capability_policy_verified = disabled_network_process.capability_policy_verified
        && controlled_network_process.capability_policy_verified;
    drop(profile);
    let shell = Path::new(&system_root).join("System32").join("cmd.exe");
    let runtime_environment = restricted_runtime_environment();
    let runtime_output = lease_manager.run_workspace_command(
        workspace.path(),
        &shell,
        &[
            "/d".to_owned(),
            "/c".to_owned(),
            "echo captured-launcher".to_owned(),
        ],
        WorkspaceAccess::ReadOnly,
        "disabled",
        &runtime_environment,
        Duration::from_secs(10),
        64 * 1_024,
    )?;
    let mut interactive = lease_manager.spawn_workspace_command(
        workspace.path(),
        &shell,
        &[
            "/d".to_owned(),
            "/q".to_owned(),
            "/v:on".to_owned(),
            "/c".to_owned(),
            "set /p CHATOS_LINE=& echo reply:!CHATOS_LINE!".to_owned(),
        ],
        WorkspaceAccess::ReadOnly,
        "disabled",
        &runtime_environment,
    )?;
    let mut interactive_stdin = interactive
        .take_stdin()
        .ok_or_else(|| "AppContainer 交互运行时 stdin 不可用。".to_owned())?;
    let mut interactive_stdout = interactive
        .take_stdout()
        .ok_or_else(|| "AppContainer 交互运行时 stdout 不可用。".to_owned())?;
    let mut interactive_stderr = interactive
        .take_stderr()
        .ok_or_else(|| "AppContainer 交互运行时 stderr 不可用。".to_owned())?;
    interactive_stdin
        .write_all(b"runtime-roundtrip\r\n")
        .map_err(|error| format!("写入 AppContainer 交互运行时失败：{error}"))?;
    drop(interactive_stdin);
    let interactive_exit = interactive.wait(Duration::from_secs(10))?;
    let mut interactive_stdout_bytes = Vec::new();
    interactive_stdout
        .read_to_end(&mut interactive_stdout_bytes)
        .map_err(|error| format!("读取 AppContainer 交互运行时 stdout 失败：{error}"))?;
    let mut interactive_stderr_bytes = Vec::new();
    interactive_stderr
        .read_to_end(&mut interactive_stderr_bytes)
        .map_err(|error| format!("读取 AppContainer 交互运行时 stderr 失败：{error}"))?;
    let runtime_launcher_verified = runtime_output.exit_code == 0
        && String::from_utf8_lossy(&runtime_output.stdout).trim() == "captured-launcher"
        && runtime_output.stderr.is_empty()
        && interactive_exit == 0
        && String::from_utf8_lossy(&interactive_stdout_bytes).trim() == "reply:runtime-roundtrip"
        && interactive_stderr_bytes.is_empty();
    Ok(AppContainerProbe {
        filesystem_denied: disabled_network_process.exit_code != 0
            && controlled_network_process.exit_code != 0,
        network_denied,
        workspace_lease_verified: granted_exit == 0 && restored_exit != 0 && !journal_path.exists(),
        workspace_recovery_verified: recovery_granted_exit == 0
            && recovered_exit != 0
            && recovered_count == 1
            && !recovery_journal_path.exists(),
        workspace_permission_tiers_verified: read_only_write_exit != 0
            && !read_only_write_file.exists()
            && read_write_exit == 0
            && read_write_created
            && after_read_write_restore_read_exit != 0
            && after_read_write_restore_exit != 0
            && !read_write_file.exists(),
        nonempty_workspace_tree_verified,
        network_capability_policy_verified,
        runtime_launcher_verified,
    })
}

fn read_probe_file(
    profile: &AppContainerProfile,
    reader: &Path,
    file: &Path,
) -> Result<u32, String> {
    let command_line = format!(
        "\"{}\" /r /c:\"lease-readable\" \"{}\"",
        reader.display(),
        file.display()
    );
    profile.run(reader, &command_line, Duration::from_secs(10))
}

fn write_probe_file(
    profile: &AppContainerProfile,
    system_root: &Path,
    file: &Path,
) -> Result<u32, String> {
    let shell = system_root.join("System32").join("cmd.exe");
    if !shell.is_file() {
        return Err("Windows cmd.exe 不存在，无法验证 AppContainer 工作区写权限。".into());
    }
    let command_line = format!(
        "\"{}\" /d /c echo lease-written>\"{}\"",
        shell.display(),
        file.display()
    );
    profile.run(&shell, &command_line, Duration::from_secs(10))
}

fn probe_network_isolation(
    profile: &AppContainerProfile,
    system_root: &Path,
) -> Result<bool, String> {
    let curl = system_root.join("System32").join("curl.exe");
    if !curl.is_file() {
        return Err("Windows curl.exe 不存在，无法验证 AppContainer 网络隔离。".into());
    }
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| format!("无法启动 AppContainer 网络探测端点：{error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("无法配置 AppContainer 网络探测端点：{error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("无法读取 AppContainer 网络探测端口：{error}"))?
        .port();
    let command_line = format!(
        "\"{}\" --silent --show-error --connect-timeout 2 --max-time 3 http://127.0.0.1:{port}/",
        curl.display()
    );
    let exit_code = profile.run(&curl, &command_line, Duration::from_secs(10))?;
    let connection_reached_host = match listener.accept() {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(error) => return Err(format!("读取 AppContainer 网络探测结果失败：{error}")),
    };
    Ok(exit_code != 0 && !connection_reached_host)
}

struct AppContainerProfile {
    _fixed_name_permit: Option<FixedProfilePermit>,
    name: Vec<u16>,
    sid: PSID,
    journal_path: Option<PathBuf>,
    delete_on_drop: bool,
}

unsafe impl Send for AppContainerProfile {}

struct FixedProfilePermit;

impl FixedProfilePermit {
    fn acquire() -> Self {
        let mut in_use = FIXED_PROFILE_IN_USE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *in_use {
            in_use = FIXED_PROFILE_AVAILABLE
                .wait(in_use)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *in_use = true;
        Self
    }
}

impl Drop for FixedProfilePermit {
    fn drop(&mut self) {
        let mut in_use = FIXED_PROFILE_IN_USE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *in_use = false;
        FIXED_PROFILE_AVAILABLE.notify_one();
    }
}

impl AppContainerProfile {
    fn create(name: &str) -> Result<Self, String> {
        let permit = FixedProfilePermit::acquire();
        let encoded_name = wide(name);
        unsafe {
            // A process termination can bypass Rust destructors. The deterministic probe
            // identity lets the next run remove a stale profile before recreating it.
            DeleteAppContainerProfile(encoded_name.as_ptr());
        }
        Self::create_named(name, None, Some(permit))
    }

    fn create_journaled(journal_root: &Path) -> Result<Self, String> {
        let id = uuid::Uuid::new_v4();
        let name = format!("ChatOS.AppContainer.Runtime.{}", id.simple());
        let journal_path = journal_root.join(format!("{id}.json"));
        write_profile_journal(&journal_path, &name)?;
        match Self::create_named(&name, Some(journal_path.clone()), None) {
            Ok(profile) => Ok(profile),
            Err(error) => {
                let _ = std::fs::remove_file(journal_path);
                Err(error)
            }
        }
    }

    fn create_named(
        name: &str,
        journal_path: Option<PathBuf>,
        fixed_name_permit: Option<FixedProfilePermit>,
    ) -> Result<Self, String> {
        let name = wide(name);
        let display_name = wide("ChatOS AppContainer Probe");
        let description = wide("Temporary ChatOS Windows isolation capability probe");
        let mut sid = null_mut();
        let result = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                display_name.as_ptr(),
                description.as_ptr(),
                null(),
                0,
                &mut sid,
            )
        };
        if result < 0 || sid.is_null() {
            return Err(format!(
                "Windows AppContainer profile creation failed (HRESULT 0x{:08X}).",
                result as u32
            ));
        }
        Ok(Self {
            _fixed_name_permit: fixed_name_permit,
            name,
            sid,
            journal_path,
            delete_on_drop: true,
        })
    }

    #[cfg(test)]
    fn abandon_for_recovery(mut self) {
        self.delete_on_drop = false;
    }

    fn run(&self, executable: &Path, command_line: &str, timeout: Duration) -> Result<u32, String> {
        self.run_with_network(executable, command_line, timeout, NetworkAccess::Disabled)
            .map(|result| result.exit_code)
    }

    fn run_with_network(
        &self,
        executable: &Path,
        command_line: &str,
        timeout: Duration,
        network_access: NetworkAccess,
    ) -> Result<AppContainerProcessResult, String> {
        let executable = wide_os(executable.as_os_str());
        let mut command_line = wide(command_line);
        let mut attribute_list = AttributeList::new()?;
        let mut capability_set = CapabilitySet::for_network_access(network_access)?;
        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: self.sid,
            Capabilities: capability_set.pointer(),
            CapabilityCount: capability_set.attributes.len() as u32,
            Reserved: 0,
        };
        attribute_list.set_security_capabilities(&capabilities)?;

        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = attribute_list.pointer;
        let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
        let created = unsafe {
            CreateProcessW(
                executable.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW | CREATE_SUSPENDED,
                null(),
                null(),
                &startup.StartupInfo,
                &mut process,
            )
        };
        if created == 0 {
            return Err(last_error("Windows AppContainer process creation failed"));
        }
        let process = ProcessHandles::new(process);
        let capability_policy_verified =
            match process_capabilities_match(process.process, capability_set.expected_sid()) {
                Ok(true) => true,
                Ok(false) => {
                    terminate_suspended_process(&process);
                    return Err(
                        "The AppContainer process capability policy did not match.".to_owned()
                    );
                }
                Err(error) => {
                    terminate_suspended_process(&process);
                    return Err(error);
                }
            };
        if unsafe { ResumeThread(process.thread) } == u32::MAX {
            terminate_suspended_process(&process);
            return Err(last_error(
                "Resuming the verified AppContainer process failed",
            ));
        }
        let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        match unsafe { WaitForSingleObject(process.process, timeout_ms) } {
            WAIT_OBJECT_0 => {}
            WAIT_TIMEOUT => {
                unsafe {
                    TerminateProcess(process.process, 124);
                    WaitForSingleObject(process.process, 5_000);
                }
                return Err("Windows AppContainer probe timed out.".into());
            }
            _ => {
                return Err(last_error(
                    "Waiting for Windows AppContainer process failed",
                ))
            }
        }
        let mut exit_code = 0;
        if unsafe { GetExitCodeProcess(process.process, &mut exit_code) } == 0 {
            return Err(last_error("Reading Windows AppContainer exit code failed"));
        }
        Ok(AppContainerProcessResult {
            exit_code,
            capability_policy_verified,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn run_captured(
        &self,
        executable: &Path,
        command_line: &[u16],
        current_directory: &Path,
        timeout: Duration,
        network_access: NetworkAccess,
        environment: Option<&mut [u16]>,
        maximum_output_bytes: usize,
    ) -> Result<AppContainerCapturedProcessResult, String> {
        let executable = wide_os(executable.as_os_str());
        let mut command_line = command_line.to_vec();
        if command_line.last().copied() != Some(0) {
            return Err("The captured AppContainer command line is invalid.".to_owned());
        }
        let current_directory = process_directory_wide(current_directory)?;
        let mut attribute_list = AttributeList::with_capacity(2)?;
        let mut capability_set = CapabilitySet::for_network_access(network_access)?;
        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: self.sid,
            Capabilities: capability_set.pointer(),
            CapabilityCount: capability_set.attributes.len() as u32,
            Reserved: 0,
        };
        attribute_list.set_security_capabilities(&capabilities)?;

        let stdin_pipe = InheritedPipe::input()?;
        let stdout_pipe = InheritedPipe::output()?;
        let stderr_pipe = InheritedPipe::output()?;
        let mut inherited_handles = [stdin_pipe.child.0, stdout_pipe.child.0, stderr_pipe.child.0];
        attribute_list.set_inherited_handles(&mut inherited_handles)?;

        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = stdin_pipe.child.0;
        startup.StartupInfo.hStdOutput = stdout_pipe.child.0;
        startup.StartupInfo.hStdError = stderr_pipe.child.0;
        startup.lpAttributeList = attribute_list.pointer;
        let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
        let environment_pointer = environment
            .as_ref()
            .map_or(null(), |block| block.as_ptr().cast::<c_void>());
        let environment_flag = if environment.is_some() {
            CREATE_UNICODE_ENVIRONMENT
        } else {
            0
        };
        let created = unsafe {
            CreateProcessW(
                executable.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT
                    | CREATE_NO_WINDOW
                    | CREATE_SUSPENDED
                    | environment_flag,
                environment_pointer,
                current_directory.as_ptr(),
                &startup.StartupInfo,
                &mut process,
            )
        };
        if created == 0 {
            return Err(last_error(
                "Windows captured AppContainer process creation failed",
            ));
        }
        let process = ProcessHandles::new(process);
        match process_capabilities_match(process.process, capability_set.expected_sid()) {
            Ok(true) => {}
            Ok(false) => {
                terminate_suspended_process(&process);
                return Err(
                    "The captured AppContainer process capability policy did not match.".to_owned(),
                );
            }
            Err(error) => {
                terminate_suspended_process(&process);
                return Err(error);
            }
        }
        let termination = ProcessTermination::attach_raw_handle(process.process as RawHandle)
            .map_err(|error| {
                terminate_suspended_process(&process);
                format!("Isolating the captured AppContainer process tree failed: {error}")
            })?;

        drop(stdin_pipe.parent);
        drop(stdin_pipe.child);
        drop(stdout_pipe.child);
        drop(stderr_pipe.child);
        let total = Arc::new(AtomicUsize::new(0));
        let exceeded = Arc::new(AtomicBool::new(false));
        let stdout_total = total.clone();
        let stdout_exceeded = exceeded.clone();
        let stdout_reader = thread::spawn(move || {
            read_bounded_pipe(
                stdout_pipe.parent,
                stdout_total,
                stdout_exceeded,
                maximum_output_bytes,
            )
        });
        let stderr_total = total;
        let stderr_exceeded = exceeded.clone();
        let stderr_reader = thread::spawn(move || {
            read_bounded_pipe(
                stderr_pipe.parent,
                stderr_total,
                stderr_exceeded,
                maximum_output_bytes,
            )
        });

        if unsafe { ResumeThread(process.thread) } == u32::MAX {
            terminate_suspended_process(&process);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(last_error(
                "Resuming the captured AppContainer process failed",
            ));
        }
        let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        let wait_error = match unsafe { WaitForSingleObject(process.process, timeout_ms) } {
            WAIT_OBJECT_0 => None,
            WAIT_TIMEOUT => {
                let _ = termination.terminate(true);
                unsafe { WaitForSingleObject(process.process, 5_000) };
                Some("The captured AppContainer process timed out.".to_owned())
            }
            _ => Some(last_error(
                "Waiting for the captured AppContainer process failed",
            )),
        };
        let _ = termination.terminate(true);
        let stdout = stdout_reader
            .join()
            .map_err(|_| "The AppContainer stdout reader terminated unexpectedly.".to_owned())??;
        let stderr = stderr_reader
            .join()
            .map_err(|_| "The AppContainer stderr reader terminated unexpectedly.".to_owned())??;
        if let Some(error) = wait_error {
            return Err(error);
        }
        if exceeded.load(Ordering::Acquire) {
            return Err("The captured AppContainer process output exceeded its limit.".to_owned());
        }
        let mut exit_code = 0;
        if unsafe { GetExitCodeProcess(process.process, &mut exit_code) } == 0 {
            return Err(last_error(
                "Reading the captured AppContainer process exit code failed",
            ));
        }
        Ok(AppContainerCapturedProcessResult {
            exit_code,
            stdout,
            stderr,
            capability_policy_verified: true,
        })
    }

    fn spawn_interactive(
        &self,
        executable: &Path,
        command_line: &[u16],
        current_directory: &Path,
        network_access: NetworkAccess,
        environment: Option<&mut [u16]>,
    ) -> Result<AppContainerInteractiveProcess, String> {
        let executable = wide_os(executable.as_os_str());
        let mut command_line = command_line.to_vec();
        if command_line.last().copied() != Some(0) {
            return Err("The interactive AppContainer command line is invalid.".to_owned());
        }
        let current_directory = process_directory_wide(current_directory)?;
        let mut attribute_list = AttributeList::with_capacity(2)?;
        let mut capability_set = CapabilitySet::for_network_access(network_access)?;
        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: self.sid,
            Capabilities: capability_set.pointer(),
            CapabilityCount: capability_set.attributes.len() as u32,
            Reserved: 0,
        };
        attribute_list.set_security_capabilities(&capabilities)?;

        let stdin_pipe = InheritedPipe::input()?;
        let stdout_pipe = InheritedPipe::output()?;
        let stderr_pipe = InheritedPipe::output()?;
        let mut inherited_handles = [stdin_pipe.child.0, stdout_pipe.child.0, stderr_pipe.child.0];
        attribute_list.set_inherited_handles(&mut inherited_handles)?;

        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = stdin_pipe.child.0;
        startup.StartupInfo.hStdOutput = stdout_pipe.child.0;
        startup.StartupInfo.hStdError = stderr_pipe.child.0;
        startup.lpAttributeList = attribute_list.pointer;
        let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
        let environment_pointer = environment
            .as_ref()
            .map_or(null(), |block| block.as_ptr().cast::<c_void>());
        let environment_flag = if environment.is_some() {
            CREATE_UNICODE_ENVIRONMENT
        } else {
            0
        };
        let created = unsafe {
            CreateProcessW(
                executable.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT
                    | CREATE_NO_WINDOW
                    | CREATE_SUSPENDED
                    | environment_flag,
                environment_pointer,
                current_directory.as_ptr(),
                &startup.StartupInfo,
                &mut process,
            )
        };
        if created == 0 {
            return Err(last_error(
                "Windows interactive AppContainer process creation failed",
            ));
        }
        let process = ProcessHandles::new(process);
        match process_capabilities_match(process.process, capability_set.expected_sid()) {
            Ok(true) => {}
            Ok(false) => {
                terminate_suspended_process(&process);
                return Err(
                    "The interactive AppContainer process capability policy did not match."
                        .to_owned(),
                );
            }
            Err(error) => {
                terminate_suspended_process(&process);
                return Err(error);
            }
        }
        let termination = ProcessTermination::attach_raw_handle(process.process as RawHandle)
            .map_err(|error| {
                terminate_suspended_process(&process);
                format!("Isolating the interactive AppContainer process tree failed: {error}")
            })?;

        let InheritedPipe {
            parent: stdin_parent,
            child: stdin_child,
        } = stdin_pipe;
        let InheritedPipe {
            parent: stdout_parent,
            child: stdout_child,
        } = stdout_pipe;
        let InheritedPipe {
            parent: stderr_parent,
            child: stderr_child,
        } = stderr_pipe;
        drop(stdin_child);
        drop(stdout_child);
        drop(stderr_child);
        let stdin = stdin_parent.into_file();
        let stdout = stdout_parent.into_file();
        let stderr = stderr_parent.into_file();

        if unsafe { ResumeThread(process.thread) } == u32::MAX {
            terminate_suspended_process(&process);
            return Err(last_error(
                "Resuming the interactive AppContainer process failed",
            ));
        }
        Ok(AppContainerInteractiveProcess {
            termination,
            process,
            stdin,
            stdout,
            stderr,
        })
    }
}

fn terminate_suspended_process(process: &ProcessHandles) {
    unsafe {
        TerminateProcess(process.process, 125);
        WaitForSingleObject(process.process, 5_000);
    }
}

fn process_capabilities_match(process: HANDLE, expected: Option<PSID>) -> Result<bool, String> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(last_error("Opening the AppContainer process token failed"));
    }
    let token = OwnedHandle(token);
    let mut required = 0;
    unsafe {
        GetTokenInformation(token.0, TokenCapabilities, null_mut(), 0, &mut required);
    }
    if required < size_of::<u32>() as u32 {
        return Err(last_error("Sizing AppContainer token capabilities failed"));
    }
    let buffer_size = required.max(size_of::<TOKEN_GROUPS>() as u32);
    let words = (buffer_size as usize).div_ceil(size_of::<usize>());
    let mut storage = vec![0usize; words];
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenCapabilities,
            storage.as_mut_ptr().cast(),
            buffer_size,
            &mut required,
        )
    } == 0
    {
        return Err(last_error("Reading AppContainer token capabilities failed"));
    }
    let groups = unsafe { &*storage.as_ptr().cast::<TOKEN_GROUPS>() };
    let count = groups.GroupCount as usize;
    if count == 0 {
        return Ok(expected.is_none());
    }
    let groups_offset = std::mem::offset_of!(TOKEN_GROUPS, Groups);
    let required_bytes = groups_offset
        .checked_add(count.saturating_mul(size_of::<SID_AND_ATTRIBUTES>()))
        .ok_or_else(|| "The AppContainer token capability list is invalid.".to_owned())?;
    if required_bytes > required as usize || count > 64 {
        return Err("The AppContainer token capability list is invalid.".to_owned());
    }
    let entries = unsafe {
        std::slice::from_raw_parts(
            storage
                .as_ptr()
                .cast::<u8>()
                .add(groups_offset)
                .cast::<SID_AND_ATTRIBUTES>(),
            count,
        )
    };
    Ok(match expected {
        None => entries.is_empty(),
        Some(expected) => {
            entries.len() == 1
                && unsafe { windows_sys::Win32::Security::EqualSid(entries[0].Sid, expected) } != 0
        }
    })
}

struct DirectoryLease {
    workspace: PathBuf,
    journal_path: PathBuf,
    active_workspaces: Arc<Mutex<HashSet<PathBuf>>>,
    entries: Vec<LiveLeaseEntry>,
    sid_sddl: String,
    restored: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FileIdentity {
    volume_serial_number: u32,
    file_index: u64,
}

struct LeaseTreeHandle {
    path: PathBuf,
    is_directory: bool,
    identity: FileIdentity,
    file: File,
}

struct LiveLeaseEntry {
    original_dacl_sddl: String,
    identity: FileIdentity,
    file: File,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct DirectoryLeaseJournal {
    version: u32,
    access: WorkspaceAccess,
    workspace_path_utf16: Vec<u16>,
    #[serde(default)]
    original_dacl_sddl: String,
    #[serde(default)]
    sid_sddl: String,
    #[serde(default)]
    entries: Vec<DirectoryLeaseJournalEntry>,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct DirectoryLeaseJournalEntry {
    relative_path_utf16: Vec<u16>,
    original_dacl_sddl: String,
    is_directory: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    volume_serial_number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_index: Option<u64>,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ProfileLeaseJournal {
    version: u32,
    profile_name: String,
}

impl DirectoryLease {
    fn grant(
        path: &Path,
        sid: PSID,
        access: WorkspaceAccess,
        journal_path: &Path,
        active_workspaces: Arc<Mutex<HashSet<PathBuf>>>,
    ) -> Result<Self, String> {
        if journal_path.exists() {
            Self::recover(journal_path)?;
        }
        let tree = lease_tree_handles(path)?;
        let mut entries = Vec::with_capacity(tree.len());
        for entry in &tree {
            let relative = entry
                .path
                .strip_prefix(path)
                .map_err(|_| "The workspace ACL lease tree escaped its root.".to_owned())?;
            entries.push(DirectoryLeaseJournalEntry {
                relative_path_utf16: relative.as_os_str().encode_wide().collect(),
                original_dacl_sddl: read_dacl_sddl_handle(&entry.file)?,
                is_directory: entry.is_directory,
                volume_serial_number: Some(entry.identity.volume_serial_number),
                file_index: Some(entry.identity.file_index),
            });
        }
        let sid_sddl = sid_to_sddl(sid)?;
        let journal = DirectoryLeaseJournal {
            version: 3,
            access,
            workspace_path_utf16: path.as_os_str().encode_wide().collect(),
            original_dacl_sddl: String::new(),
            sid_sddl: sid_sddl.clone(),
            entries,
        };
        write_lease_journal(journal_path, &journal)?;

        for (granted, entry) in tree.iter().enumerate() {
            if let Err(failure) =
                grant_sid_access_handle(&entry.file, sid, access, entry.is_directory)
            {
                let restore_result = tree[..granted]
                    .iter()
                    .zip(journal.entries[..granted].iter())
                    .try_for_each(|(tree_entry, journal_entry)| {
                        restore_dacl_from_sddl_handle(
                            &tree_entry.file,
                            &journal_entry.original_dacl_sddl,
                        )
                    });
                if restore_result.is_ok() {
                    let _ = std::fs::remove_file(journal_path);
                    return Err(failure);
                }
                return Err(format!(
                    "{failure} Restoring the partial workspace ACL lease also failed: {}",
                    restore_result.unwrap_err()
                ));
            }
        }
        let live_entries = tree
            .into_iter()
            .zip(journal.entries)
            .map(|(tree_entry, journal_entry)| LiveLeaseEntry {
                original_dacl_sddl: journal_entry.original_dacl_sddl,
                identity: tree_entry.identity,
                file: tree_entry.file,
            })
            .collect();
        Ok(Self {
            workspace: path.to_owned(),
            journal_path: journal_path.to_owned(),
            active_workspaces,
            entries: live_entries,
            sid_sddl,
            restored: false,
        })
    }

    fn recover(journal_path: &Path) -> Result<(), String> {
        restore_lease_journal(journal_path)
    }

    fn restore(mut self) -> Result<(), String> {
        self.restore_inner()
    }

    fn abandon_for_recovery(mut self) {
        self.restored = true;
        self.release_active_workspace();
    }

    fn restore_inner(&mut self) -> Result<(), String> {
        if self.restored {
            return Ok(());
        }
        for entry in &self.entries {
            restore_dacl_from_sddl_handle(&entry.file, &entry.original_dacl_sddl)?;
        }
        let original_identities = self
            .entries
            .iter()
            .map(|entry| entry.identity)
            .collect::<HashSet<_>>();
        let sid = LocalSid::from_sddl(&self.sid_sddl)?;
        let root = self
            .entries
            .first()
            .ok_or_else(|| "The live workspace ACL lease has no root handle.".to_owned())?;
        let current_workspace = final_path_from_handle(&root.file)?;
        let current = lease_tree_handles(&current_workspace)?;
        for entry in current {
            if !original_identities.contains(&entry.identity) {
                revoke_sid_access_handle(&entry.file, sid.0, entry.is_directory)?;
            }
        }
        std::fs::remove_file(&self.journal_path).map_err(|error| {
            format!("Removing the restored workspace ACL journal failed: {error}")
        })?;
        self.restored = true;
        self.release_active_workspace();
        Ok(())
    }

    fn release_active_workspace(&self) {
        if let Ok(mut active) = self.active_workspaces.lock() {
            active.remove(&self.workspace);
        }
    }
}

impl Drop for DirectoryLease {
    fn drop(&mut self) {
        let _ = self.restore_inner();
    }
}

fn restore_directory_leases(mut leases: Vec<DirectoryLease>) -> Result<(), String> {
    let mut failures = Vec::new();
    while let Some(lease) = leases.pop() {
        if let Err(error) = lease.restore() {
            failures.push(error);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join(" "))
    }
}

fn restore_lease_journal(journal_path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(journal_path)
        .map_err(|error| format!("Reading the workspace ACL journal failed: {error}"))?;
    if bytes.len() > MAXIMUM_LEASE_JOURNAL_BYTES {
        return Err("The workspace ACL journal exceeds its safety limit.".to_owned());
    }
    let journal: DirectoryLeaseJournal = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Parsing the workspace ACL journal failed: {error}"))?;
    match journal.version {
        1 => {
            if journal.workspace_path_utf16.is_empty()
                || journal.workspace_path_utf16.contains(&0)
                || journal.original_dacl_sddl.is_empty()
                || !journal.entries.is_empty()
                || !journal.sid_sddl.is_empty()
            {
                return Err("The workspace ACL journal is invalid.".to_owned());
            }
            let workspace = PathBuf::from(OsString::from_wide(&journal.workspace_path_utf16));
            if !workspace.is_absolute() {
                return Err("The workspace ACL journal root is invalid.".to_owned());
            }
            match std::fs::symlink_metadata(&workspace) {
                Ok(metadata) => {
                    if !metadata.is_dir() || is_reparse_point(&metadata) {
                        return Err("The workspace ACL journal root is invalid.".to_owned());
                    }
                    restore_dacl_from_sddl(&workspace, &journal.original_dacl_sddl)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Inspecting the workspace ACL journal root failed: {error}"
                    ))
                }
            }
        }
        2 | 3 => restore_tree_lease(&journal)?,
        _ => return Err("The workspace ACL journal version is unsupported.".to_owned()),
    }
    std::fs::remove_file(journal_path)
        .map_err(|error| format!("Removing the restored workspace ACL journal failed: {error}"))
}

fn restore_tree_lease(journal: &DirectoryLeaseJournal) -> Result<(), String> {
    if journal.workspace_path_utf16.is_empty()
        || journal.workspace_path_utf16.contains(&0)
        || journal.sid_sddl.is_empty()
        || !journal.original_dacl_sddl.is_empty()
        || journal.entries.is_empty()
        || journal.entries.len() > MAXIMUM_LEASE_TREE_ENTRIES
    {
        return Err("The workspace ACL tree journal is invalid.".to_owned());
    }
    let workspace = PathBuf::from(OsString::from_wide(&journal.workspace_path_utf16));
    if !workspace.is_absolute() {
        return Err("The workspace ACL tree journal root is invalid.".to_owned());
    }
    match std::fs::symlink_metadata(&workspace) {
        Ok(metadata) => {
            if !metadata.is_dir() || is_reparse_point(&metadata) {
                return Err("The workspace ACL tree journal root is invalid.".to_owned());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Inspecting the workspace ACL recovery root failed: {error}"
            ))
        }
    }
    let workspace = std::fs::canonicalize(&workspace)
        .map_err(|error| format!("Resolving the workspace ACL recovery root failed: {error}"))?;
    let sid = LocalSid::from_sddl(&journal.sid_sddl)?;
    let mut original_paths = HashSet::with_capacity(journal.entries.len());
    let mut restored_entries = Vec::with_capacity(journal.entries.len());
    for entry in &journal.entries {
        if entry.original_dacl_sddl.is_empty() || entry.relative_path_utf16.contains(&0) {
            return Err("The workspace ACL tree journal entry is invalid.".to_owned());
        }
        let relative = PathBuf::from(OsString::from_wide(&entry.relative_path_utf16));
        if !safe_relative_lease_path(&relative) {
            return Err("The workspace ACL tree journal path is invalid.".to_owned());
        }
        if !original_paths.insert(relative.clone()) {
            return Err("The workspace ACL tree journal contains duplicate paths.".to_owned());
        }
        if journal.version == 3
            && (entry.volume_serial_number.is_none() || entry.file_index.is_none())
        {
            return Err("The workspace ACL tree journal identity is invalid.".to_owned());
        }
        restored_entries.push((relative, entry));
    }
    if !original_paths.contains(Path::new("")) {
        return Err("The workspace ACL tree journal does not contain its root.".to_owned());
    }
    if !journal
        .entries
        .iter()
        .any(|entry| entry.relative_path_utf16.is_empty() && entry.is_directory)
    {
        return Err("The workspace ACL tree journal root entry is invalid.".to_owned());
    }

    let current_before_restore = lease_tree_handles(&workspace)?;
    let mut current_by_path = current_before_restore
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            entry
                .path
                .strip_prefix(&workspace)
                .map(|relative| (relative.to_owned(), index))
                .map_err(|_| "The workspace ACL recovery tree escaped its root.".to_owned())
        })
        .collect::<Result<HashMap<_, _>, _>>()?;

    let mut matched_entries = Vec::with_capacity(restored_entries.len());
    for (relative, journal_entry) in &restored_entries {
        if let Some(index) = current_by_path.remove(relative) {
            let current = &current_before_restore[index];
            if current.is_directory != journal_entry.is_directory {
                return Err("A workspace ACL recovery entry changed type.".to_owned());
            }
            if journal.version == 3
                && current.identity
                    != (FileIdentity {
                        volume_serial_number: journal_entry.volume_serial_number.unwrap(),
                        file_index: journal_entry.file_index.unwrap(),
                    })
            {
                return Err("A workspace ACL recovery entry changed identity.".to_owned());
            }
            matched_entries.push((index, *journal_entry));
        }
    }

    for (index, journal_entry) in matched_entries {
        restore_dacl_from_sddl_handle(
            &current_before_restore[index].file,
            &journal_entry.original_dacl_sddl,
        )?;
    }

    for (_, index) in current_by_path {
        let current = &current_before_restore[index];
        let relative = current
            .path
            .strip_prefix(&workspace)
            .map_err(|_| "The workspace ACL recovery tree escaped its root.".to_owned())?;
        if !original_paths.contains(relative) {
            revoke_sid_access_handle(&current.file, sid.0, current.is_directory)?;
        }
    }
    Ok(())
}

fn safe_relative_lease_path(path: &Path) -> bool {
    path.as_os_str().is_empty()
        || (!path.is_absolute()
            && path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))))
}

fn lease_tree_handles(root: &Path) -> Result<Vec<LeaseTreeHandle>, String> {
    let root_entry = open_acl_handle(root)?;
    if !root_entry.is_directory {
        return Err("The workspace ACL tree root is invalid.".to_owned());
    }
    let root_final_path = final_path_from_handle(&root_entry.file)?;
    let mut result = vec![root_entry];
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let mut children = std::fs::read_dir(&directory)
            .map_err(|error| format!("Reading the workspace ACL tree failed: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Reading the workspace ACL tree failed: {error}"))?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let entry = open_acl_handle(&path)?;
            let final_path = final_path_from_handle(&entry.file)?;
            if !final_path.starts_with(&root_final_path) || final_path == root_final_path {
                return Err("The workspace ACL tree handle escaped its root.".to_owned());
            }
            let is_directory = entry.is_directory;
            result.push(entry);
            if result.len() > MAXIMUM_LEASE_TREE_ENTRIES {
                return Err("The workspace ACL tree exceeds its entry limit.".to_owned());
            }
            if is_directory {
                pending.push(path);
            }
        }
    }
    result.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| {
                left.path
                    .components()
                    .count()
                    .cmp(&right.path.components().count())
            })
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(result)
}

fn open_acl_handle(path: &Path) -> Result<LeaseTreeHandle, String> {
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| format!("Opening a workspace ACL tree handle failed: {error}"))?;
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    if unsafe { GetFileInformationByHandle(file_handle(&file), &mut information) } == 0 {
        return Err(last_error("Inspecting a workspace ACL tree handle failed"));
    }
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err("The workspace ACL tree contains a reparse point.".to_owned());
    }
    let is_directory = information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    Ok(LeaseTreeHandle {
        path: path.to_owned(),
        is_directory,
        identity: FileIdentity {
            volume_serial_number: information.dwVolumeSerialNumber,
            file_index: ((information.nFileIndexHigh as u64) << 32)
                | information.nFileIndexLow as u64,
        },
        file,
    })
}

fn file_handle(file: &File) -> HANDLE {
    file.as_raw_handle() as RawHandle as HANDLE
}

fn final_path_from_handle(file: &File) -> Result<PathBuf, String> {
    let required = unsafe { GetFinalPathNameByHandleW(file_handle(file), null_mut(), 0, 0) };
    if required == 0 || required > 32_767 {
        return Err(last_error("Resolving a workspace ACL tree handle failed"));
    }
    let mut buffer = vec![0u16; required as usize + 1];
    let written = unsafe {
        GetFinalPathNameByHandleW(
            file_handle(file),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if written == 0 || written as usize >= buffer.len() {
        return Err(last_error("Resolving a workspace ACL tree handle failed"));
    }
    buffer.truncate(written as usize);
    Ok(PathBuf::from(OsString::from_wide(&buffer)))
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(test)]
fn read_dacl_sddl(path: &Path) -> Result<String, String> {
    let path = wide_os(path.as_os_str());
    let mut descriptor = null_mut();
    let result = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 || descriptor.is_null() {
        return Err(format!(
            "Reading a workspace ACL tree entry failed (Win32 error {result})."
        ));
    }
    let sddl = descriptor_to_sddl(descriptor);
    unsafe {
        LocalFree(descriptor.cast());
    }
    sddl
}

fn read_dacl_sddl_handle(file: &File) -> Result<String, String> {
    let mut descriptor = null_mut();
    let result = unsafe {
        GetSecurityInfo(
            file_handle(file),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 || descriptor.is_null() {
        return Err(format!(
            "Reading a workspace ACL tree handle failed (Win32 error {result})."
        ));
    }
    let sddl = descriptor_to_sddl(descriptor);
    unsafe {
        LocalFree(descriptor.cast());
    }
    sddl
}

#[cfg(test)]
fn grant_sid_access(
    path: &Path,
    sid: PSID,
    access: WorkspaceAccess,
    is_directory: bool,
) -> Result<(), String> {
    change_sid_access(path, sid, Some(access), is_directory, GRANT_ACCESS)
}

#[cfg(test)]
fn change_sid_access(
    path: &Path,
    sid: PSID,
    access: Option<WorkspaceAccess>,
    is_directory: bool,
    mode: i32,
) -> Result<(), String> {
    let mut path = wide_os(path.as_os_str());
    let mut original_dacl = null_mut();
    let mut descriptor = null_mut();
    let read_result = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut original_dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if read_result != 0 || descriptor.is_null() {
        return Err(format!(
            "Reading a workspace ACL entry failed (Win32 error {read_result})."
        ));
    }
    let mut entry: EXPLICIT_ACCESS_W = unsafe { zeroed() };
    entry.grfAccessPermissions = match access {
        Some(WorkspaceAccess::ReadOnly) => FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
        Some(WorkspaceAccess::ReadWrite) => {
            FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | FILE_DELETE_CHILD
        }
        None => 0,
    };
    entry.grfAccessMode = mode;
    entry.grfInheritance = if is_directory {
        SUB_CONTAINERS_AND_OBJECTS_INHERIT
    } else {
        0
    };
    entry.Trustee.TrusteeForm = TRUSTEE_IS_SID;
    entry.Trustee.TrusteeType = TRUSTEE_IS_WELL_KNOWN_GROUP;
    entry.Trustee.ptstrName = sid.cast();
    let mut updated_dacl = null_mut();
    let update_result = unsafe { SetEntriesInAclW(1, &entry, original_dacl, &mut updated_dacl) };
    if update_result != 0 || updated_dacl.is_null() {
        unsafe {
            LocalFree(descriptor.cast());
        }
        return Err(format!(
            "Building a workspace ACL entry failed (Win32 error {update_result})."
        ));
    }
    let apply_result = unsafe {
        SetNamedSecurityInfoW(
            path.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            updated_dacl,
            null(),
        )
    };
    unsafe {
        LocalFree(updated_dacl.cast());
        LocalFree(descriptor.cast());
    }
    if apply_result != 0 {
        Err(format!(
            "Applying a workspace ACL entry failed (Win32 error {apply_result})."
        ))
    } else {
        Ok(())
    }
}

fn grant_sid_access_handle(
    file: &File,
    sid: PSID,
    access: WorkspaceAccess,
    is_directory: bool,
) -> Result<(), String> {
    change_sid_access_handle(file, sid, Some(access), is_directory, GRANT_ACCESS)
}

fn revoke_sid_access_handle(file: &File, sid: PSID, is_directory: bool) -> Result<(), String> {
    change_sid_access_handle(file, sid, None, is_directory, REVOKE_ACCESS)
}

fn change_sid_access_handle(
    file: &File,
    sid: PSID,
    access: Option<WorkspaceAccess>,
    is_directory: bool,
    mode: i32,
) -> Result<(), String> {
    let mut original_dacl = null_mut();
    let mut descriptor = null_mut();
    let read_result = unsafe {
        GetSecurityInfo(
            file_handle(file),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut original_dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if read_result != 0 || descriptor.is_null() {
        return Err(format!(
            "Reading a workspace ACL handle failed (Win32 error {read_result})."
        ));
    }
    let mut entry: EXPLICIT_ACCESS_W = unsafe { zeroed() };
    entry.grfAccessPermissions = match access {
        Some(WorkspaceAccess::ReadOnly) => FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
        Some(WorkspaceAccess::ReadWrite) => {
            FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | FILE_DELETE_CHILD
        }
        None => 0,
    };
    entry.grfAccessMode = mode;
    entry.grfInheritance = if is_directory {
        SUB_CONTAINERS_AND_OBJECTS_INHERIT
    } else {
        0
    };
    entry.Trustee.TrusteeForm = TRUSTEE_IS_SID;
    entry.Trustee.TrusteeType = TRUSTEE_IS_WELL_KNOWN_GROUP;
    entry.Trustee.ptstrName = sid.cast();
    let mut updated_dacl = null_mut();
    let update_result = unsafe { SetEntriesInAclW(1, &entry, original_dacl, &mut updated_dacl) };
    if update_result != 0 || updated_dacl.is_null() {
        unsafe {
            LocalFree(descriptor.cast());
        }
        return Err(format!(
            "Building a workspace ACL handle failed (Win32 error {update_result})."
        ));
    }
    let apply_result = unsafe {
        SetSecurityInfo(
            file_handle(file),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            updated_dacl,
            null(),
        )
    };
    unsafe {
        LocalFree(updated_dacl.cast());
        LocalFree(descriptor.cast());
    }
    if apply_result != 0 {
        Err(format!(
            "Applying a workspace ACL handle failed (Win32 error {apply_result})."
        ))
    } else {
        Ok(())
    }
}

fn sid_to_sddl(sid: PSID) -> Result<String, String> {
    let mut value = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut value) } == 0 || value.is_null() {
        return Err(last_error("Encoding the workspace ACL SID failed"));
    }
    let mut length = 0usize;
    while length < 256 && unsafe { *value.add(length) } != 0 {
        length += 1;
    }
    if length == 256 {
        unsafe {
            LocalFree(value.cast());
        }
        return Err("The workspace ACL SID is invalid.".to_owned());
    }
    let result = String::from_utf16(unsafe { std::slice::from_raw_parts(value, length) })
        .map_err(|_| "The workspace ACL SID is invalid.".to_owned());
    unsafe {
        LocalFree(value.cast());
    }
    result
}

struct LocalSid(PSID);

impl LocalSid {
    fn from_sddl(value: &str) -> Result<Self, String> {
        if !value.starts_with("S-1-15-2-")
            || value.len() > 256
            || value[1..]
                .chars()
                .any(|character| !character.is_ascii_digit() && character != '-')
        {
            return Err("The workspace ACL SID is invalid.".to_owned());
        }
        let value = wide(value);
        let mut sid = null_mut();
        if unsafe { ConvertStringSidToSidW(value.as_ptr(), &mut sid) } == 0 || sid.is_null() {
            return Err(last_error("Decoding the workspace ACL SID failed"));
        }
        Ok(Self(sid))
    }
}

impl Drop for LocalSid {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0.cast());
        }
    }
}

fn descriptor_to_sddl(descriptor: PSECURITY_DESCRIPTOR) -> Result<String, String> {
    let mut value = null_mut();
    let mut length = 0;
    let converted = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut value,
            &mut length,
        )
    };
    if converted == 0 || value.is_null() {
        return Err(last_error(
            "Converting the workspace ACL to a recovery journal failed",
        ));
    }
    let text = String::from_utf16(unsafe { std::slice::from_raw_parts(value, length as usize) })
        .map_err(|_| "The workspace ACL recovery descriptor is invalid UTF-16.".to_owned());
    unsafe {
        LocalFree(value.cast());
    }
    text
}

fn restore_dacl_from_sddl(path: &Path, sddl: &str) -> Result<(), String> {
    let sddl = wide(sddl);
    let mut descriptor = null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    };
    if converted == 0 || descriptor.is_null() {
        return Err(last_error(
            "Reading the workspace ACL recovery descriptor failed",
        ));
    }
    let path = wide_os(path.as_os_str());
    let restored =
        unsafe { SetFileSecurityW(path.as_ptr(), DACL_SECURITY_INFORMATION, descriptor) };
    unsafe {
        LocalFree(descriptor.cast());
    }
    if restored == 0 {
        Err(last_error(
            "Restoring the workspace ACL from its journal failed",
        ))
    } else {
        Ok(())
    }
}

fn restore_dacl_from_sddl_handle(file: &File, sddl: &str) -> Result<(), String> {
    let sddl = wide(sddl);
    let mut descriptor = null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    };
    if converted == 0 || descriptor.is_null() {
        return Err(last_error(
            "Reading the workspace ACL recovery descriptor failed",
        ));
    }
    let mut dacl_present = 0;
    let mut dacl = null_mut();
    let mut dacl_defaulted = 0;
    let read_dacl = unsafe {
        GetSecurityDescriptorDacl(
            descriptor,
            &mut dacl_present,
            &mut dacl,
            &mut dacl_defaulted,
        )
    };
    if read_dacl == 0 || dacl_present == 0 {
        unsafe {
            LocalFree(descriptor.cast());
        }
        return Err(last_error("Reading the workspace ACL recovery DACL failed"));
    }
    let restored = unsafe {
        SetSecurityInfo(
            file_handle(file),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            dacl,
            null(),
        )
    };
    unsafe {
        LocalFree(descriptor.cast());
    }
    if restored != 0 {
        Err(format!(
            "Restoring the workspace ACL handle from its journal failed (Win32 error {restored})."
        ))
    } else {
        Ok(())
    }
}

fn write_profile_journal(journal_path: &Path, profile_name: &str) -> Result<(), String> {
    if !valid_runtime_profile_name(profile_name) {
        return Err("The AppContainer runtime profile name is invalid.".to_owned());
    }
    let payload = serde_json::to_vec(&ProfileLeaseJournal {
        version: 1,
        profile_name: profile_name.to_owned(),
    })
    .map_err(|error| format!("Encoding the AppContainer profile lease failed: {error}"))?;
    let directory = journal_path
        .parent()
        .ok_or_else(|| "The AppContainer profile lease has no parent directory.".to_owned())?;
    create_owned_file(
        directory,
        journal_path,
        PROFILE_JOURNAL_TEMPORARY_PREFIX,
        &payload,
    )
    .map_err(|error| format!("Publishing the AppContainer profile lease failed: {error}"))
}

fn recover_profile_journal(journal_path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(journal_path)
        .map_err(|error| format!("Reading the AppContainer profile lease failed: {error}"))?;
    if bytes.len() > 1_024 {
        return Err("The AppContainer profile lease exceeds its safety limit.".to_owned());
    }
    let journal: ProfileLeaseJournal = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Parsing the AppContainer profile lease failed: {error}"))?;
    if journal.version != 1 || !valid_runtime_profile_name(&journal.profile_name) {
        return Err("The AppContainer profile lease is invalid.".to_owned());
    }
    let name = wide(&journal.profile_name);
    let deleted = unsafe { DeleteAppContainerProfile(name.as_ptr()) };
    let error_code = deleted as u32 & 0xffff;
    if deleted < 0 && error_code != 2 && error_code != 1_168 {
        return Err(format!(
            "Recovering the AppContainer profile lease failed (HRESULT 0x{:08X}).",
            deleted as u32
        ));
    }
    std::fs::remove_file(journal_path)
        .map_err(|error| format!("Removing the AppContainer profile lease failed: {error}"))
}

fn valid_runtime_profile_name(value: &str) -> bool {
    let Some(suffix) = value.strip_prefix("ChatOS.AppContainer.Runtime.") else {
        return false;
    };
    suffix.len() == 32
        && suffix
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn write_lease_journal(journal_path: &Path, journal: &DirectoryLeaseJournal) -> Result<(), String> {
    let parent = journal_path
        .parent()
        .ok_or_else(|| "The workspace ACL journal has no parent directory.".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Creating the workspace ACL journal directory failed: {error}"))?;
    let payload = serde_json::to_vec(journal)
        .map_err(|error| format!("Encoding the workspace ACL journal failed: {error}"))?;
    if payload.len() > MAXIMUM_LEASE_JOURNAL_BYTES {
        return Err("The workspace ACL journal exceeds its safety limit.".to_owned());
    }
    create_owned_file(
        parent,
        journal_path,
        WORKSPACE_JOURNAL_TEMPORARY_PREFIX,
        &payload,
    )
    .map_err(|error| format!("Publishing the workspace ACL journal failed: {error}"))
}

fn canonical_uuid_name(name: &OsStr, prefix: &str, suffix: &str) -> bool {
    let Some(value) = name
        .to_str()
        .and_then(|value| value.strip_prefix(prefix))
        .and_then(|value| value.strip_suffix(suffix))
    else {
        return false;
    };
    uuid::Uuid::parse_str(value).is_ok_and(|uuid| uuid.to_string() == value)
}

impl Drop for AppContainerProfile {
    fn drop(&mut self) {
        unsafe {
            if !self.sid.is_null() {
                FreeSid(self.sid);
            }
        }
        if self.delete_on_drop {
            let deleted = unsafe { DeleteAppContainerProfile(self.name.as_ptr()) };
            if deleted >= 0 {
                if let Some(journal_path) = self.journal_path.as_ref() {
                    let _ = std::fs::remove_file(journal_path);
                }
            }
        }
    }
}

struct AttributeList {
    _storage: Vec<u8>,
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn new() -> Result<Self, String> {
        Self::with_capacity(1)
    }

    fn with_capacity(attribute_count: u32) -> Result<Self, String> {
        let mut bytes = 0usize;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), attribute_count, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(last_error(
                "Sizing the Windows process attribute list failed",
            ));
        }
        let mut storage = vec![0u8; bytes];
        let pointer = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(pointer, attribute_count, 0, &mut bytes) }
            == 0
        {
            return Err(last_error(
                "Initializing the Windows process attribute list failed",
            ));
        }
        Ok(Self {
            _storage: storage,
            pointer,
        })
    }

    fn set_security_capabilities(
        &mut self,
        capabilities: &SECURITY_CAPABILITIES,
    ) -> Result<(), String> {
        let updated = unsafe {
            UpdateProcThreadAttribute(
                self.pointer,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                (capabilities as *const SECURITY_CAPABILITIES).cast::<c_void>(),
                size_of::<SECURITY_CAPABILITIES>(),
                null_mut(),
                null(),
            )
        };
        if updated == 0 {
            Err(last_error(
                "Attaching Windows AppContainer security capabilities failed",
            ))
        } else {
            Ok(())
        }
    }

    fn set_inherited_handles(&mut self, handles: &mut [HANDLE]) -> Result<(), String> {
        let updated = unsafe {
            UpdateProcThreadAttribute(
                self.pointer,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_mut_ptr().cast::<c_void>(),
                std::mem::size_of_val(handles),
                null_mut(),
                null(),
            )
        };
        if updated == 0 {
            Err(last_error(
                "Restricting inherited AppContainer process handles failed",
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer);
        }
    }
}

struct ProcessHandles {
    process: HANDLE,
    thread: HANDLE,
}

unsafe impl Send for ProcessHandles {}

struct OwnedHandle(HANDLE);

unsafe impl Send for OwnedHandle {}

impl OwnedHandle {
    fn into_file(mut self) -> File {
        let handle = self.0;
        self.0 = null_mut();
        unsafe { File::from_raw_handle(handle as RawHandle) }
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                CloseHandle(self.0);
            }
        }
    }
}

struct InheritedPipe {
    parent: OwnedHandle,
    child: OwnedHandle,
}

impl InheritedPipe {
    fn output() -> Result<Self, String> {
        create_inherited_pipe(false)
    }

    fn input() -> Result<Self, String> {
        create_inherited_pipe(true)
    }
}

fn create_inherited_pipe(child_reads: bool) -> Result<InheritedPipe, String> {
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let mut read = null_mut();
    let mut write = null_mut();
    if unsafe { CreatePipe(&mut read, &mut write, &security, 0) } == 0 {
        return Err(last_error("Creating AppContainer process pipe failed"));
    }
    let read = OwnedHandle(read);
    let write = OwnedHandle(write);
    let (parent, child) = if child_reads {
        (write, read)
    } else {
        (read, write)
    };
    if unsafe { SetHandleInformation(parent.0, HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(last_error(
            "Protecting an AppContainer parent pipe handle failed",
        ));
    }
    Ok(InheritedPipe { parent, child })
}

fn read_bounded_pipe(
    pipe: OwnedHandle,
    total: Arc<AtomicUsize>,
    exceeded: Arc<AtomicBool>,
    maximum_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 16 * 1_024];
    loop {
        let mut read = 0u32;
        let succeeded = unsafe {
            ReadFile(
                pipe.0,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut read,
                null_mut(),
            )
        };
        if succeeded == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_BROKEN_PIPE {
                break;
            }
            return Err(format!(
                "Reading AppContainer process output failed (Win32 error {error})."
            ));
        }
        if read == 0 {
            break;
        }
        let read = read as usize;
        let previous = total.fetch_add(read, Ordering::AcqRel);
        let remaining = maximum_bytes.saturating_sub(previous);
        if read > remaining {
            exceeded.store(true, Ordering::Release);
        }
        output.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(output)
}

impl ProcessHandles {
    fn new(process: PROCESS_INFORMATION) -> Self {
        Self {
            process: process.hProcess,
            thread: process.hThread,
        }
    }
}

impl Drop for ProcessHandles {
    fn drop(&mut self) {
        unsafe {
            if !self.thread.is_null() {
                CloseHandle(self.thread);
            }
            if !self.process.is_null() {
                CloseHandle(self.process);
            }
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn build_windows_command_line(
    executable: &OsStr,
    arguments: &[String],
) -> Result<Vec<u16>, String> {
    let mut command_line = Vec::new();
    append_windows_argument(
        &mut command_line,
        &executable.encode_wide().collect::<Vec<_>>(),
    );
    for argument in arguments {
        if argument.encode_utf16().any(|unit| unit == 0) {
            return Err("The AppContainer command contains an invalid argument.".to_owned());
        }
        command_line.push(b' ' as u16);
        append_windows_argument(
            &mut command_line,
            &argument.encode_utf16().collect::<Vec<_>>(),
        );
    }
    command_line.push(0);
    if command_line.len() > 32_767 {
        return Err("The AppContainer command line exceeds its Windows limit.".to_owned());
    }
    Ok(command_line)
}

fn append_windows_argument(command_line: &mut Vec<u16>, argument: &[u16]) {
    let quote = b'"' as u16;
    let slash = b'\\' as u16;
    let needs_quotes = argument.is_empty()
        || argument
            .iter()
            .any(|unit| *unit == quote || *unit == b' ' as u16 || *unit == b'\t' as u16);
    if !needs_quotes {
        command_line.extend_from_slice(argument);
        return;
    }
    command_line.push(quote);
    let mut backslashes = 0usize;
    for unit in argument {
        if *unit == slash {
            backslashes += 1;
            continue;
        }
        if *unit == quote {
            command_line.extend(std::iter::repeat_n(slash, backslashes * 2 + 1));
            command_line.push(quote);
        } else {
            command_line.extend(std::iter::repeat_n(slash, backslashes));
            command_line.push(*unit);
        }
        backslashes = 0;
    }
    command_line.extend(std::iter::repeat_n(slash, backslashes * 2));
    command_line.push(quote);
}

fn build_environment_block(environment: &[(OsString, OsString)]) -> Result<Vec<u16>, String> {
    let mut environment = environment.to_vec();
    environment.sort_by(|left, right| {
        left.0
            .to_string_lossy()
            .to_lowercase()
            .cmp(&right.0.to_string_lossy().to_lowercase())
    });
    let mut block = Vec::new();
    let mut names = HashSet::with_capacity(environment.len());
    for (name, value) in environment {
        if !names.insert(name.to_string_lossy().to_lowercase()) {
            return Err(
                "The AppContainer process environment contains duplicate names.".to_owned(),
            );
        }
        let name = name.encode_wide().collect::<Vec<_>>();
        let value = value.encode_wide().collect::<Vec<_>>();
        if name.is_empty()
            || name.contains(&0)
            || value.contains(&0)
            || name.contains(&(b'=' as u16))
        {
            return Err("The AppContainer process environment is invalid.".to_owned());
        }
        block.extend_from_slice(&name);
        block.push(b'=' as u16);
        block.extend_from_slice(&value);
        block.push(0);
    }
    block.push(0);
    if block.len() == 1 {
        block.push(0);
    }
    if block.len() > 32_767 {
        return Err("The AppContainer process environment exceeds its Windows limit.".to_owned());
    }
    Ok(block)
}

fn restricted_runtime_environment() -> Vec<(OsString, OsString)> {
    const ALLOWED: &[&str] = &[
        "ALLUSERSPROFILE",
        "APPDATA",
        "ComSpec",
        "HOME",
        "LANG",
        "LC_ALL",
        "LOCALAPPDATA",
        "PATH",
        "PATHEXT",
        "PROGRAMDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "SystemDrive",
        "SystemRoot",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "WINDIR",
    ];
    ALLOWED
        .iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (OsString::from(name), value)))
        .collect()
}

fn wide_os(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn process_directory_wide(value: &Path) -> Result<Vec<u16>, String> {
    let mut encoded = value.as_os_str().encode_wide().collect::<Vec<_>>();
    let verbatim_prefix = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    if encoded.starts_with(&verbatim_prefix) {
        encoded.drain(..verbatim_prefix.len());
    }
    if encoded.len() < 3
        || !((b'A' as u16..=b'Z' as u16).contains(&encoded[0])
            || (b'a' as u16..=b'z' as u16).contains(&encoded[0]))
        || encoded[1] != b':' as u16
        || encoded[2] != b'\\' as u16
    {
        return Err(
            "The AppContainer process working directory must be on a local drive.".to_owned(),
        );
    }
    encoded.push(0);
    Ok(encoded)
}

fn last_error(context: &str) -> String {
    let code = unsafe { GetLastError() };
    format!("{context} (Win32 error {code}).")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn interactive_runtime_handle_can_move_to_a_blocking_worker() {
        assert_send::<SandboxedChild>();
    }

    fn read_file_exit_code(profile: &AppContainerProfile, file: &Path) -> Result<u32, String> {
        let system_root = std::env::var_os("SystemRoot")
            .ok_or_else(|| "Windows SystemRoot is unavailable.".to_owned())?;
        let reader = Path::new(&system_root).join("System32").join("findstr.exe");
        let command_line = format!(
            "\"{}\" /r /c:\"lease-readable\" \"{}\"",
            reader.display(),
            file.display()
        );
        profile.run(&reader, &command_line, Duration::from_secs(10))
    }

    #[test]
    fn launches_a_zero_capability_appcontainer_with_file_and_network_denial() {
        let probe = probe_isolation().expect("AppContainer isolation probe should run");
        assert!(probe.filesystem_denied);
        assert!(probe.network_denied);
        assert!(probe.workspace_lease_verified);
        assert!(probe.workspace_recovery_verified);
        assert!(probe.workspace_permission_tiers_verified);
        assert!(probe.nonempty_workspace_tree_verified);
        assert!(probe.network_capability_policy_verified);
        assert!(probe.runtime_launcher_verified);
        assert_eq!(
            NetworkAccess::from_policy("disabled").unwrap(),
            NetworkAccess::Disabled
        );
        assert_eq!(
            NetworkAccess::from_policy("controlled").unwrap(),
            NetworkAccess::Controlled
        );
        assert!(NetworkAccess::from_policy("host").is_err());
    }

    #[test]
    fn directory_read_lease_grants_only_the_workspace_and_restores_denial() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("outside.txt");
        std::fs::write(&outside_file, b"lease-readable").unwrap();
        let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
        let manager = WorkspaceLeaseManager::open(outside.path()).unwrap();
        let lease = manager
            .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadOnly)
            .unwrap();
        let journal_path = lease.journal_path.clone();
        assert!(manager
            .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadWrite)
            .is_err());
        let workspace_file = workspace.path().join("workspace.txt");
        std::fs::write(&workspace_file, b"lease-readable").unwrap();

        assert_eq!(read_file_exit_code(&profile, &workspace_file).unwrap(), 0);
        assert_ne!(read_file_exit_code(&profile, &outside_file).unwrap(), 0);

        lease.restore().unwrap();
        assert!(!journal_path.exists());
        assert_ne!(read_file_exit_code(&profile, &workspace_file).unwrap(), 0);

        let recovery_workspace = tempfile::tempdir().unwrap();
        let lease = manager
            .grant(
                recovery_workspace.path(),
                profile.sid,
                WorkspaceAccess::ReadOnly,
            )
            .unwrap();
        let journal_path = lease.journal_path.clone();
        let recovery_file = recovery_workspace.path().join("recovery.txt");
        std::fs::write(&recovery_file, b"lease-readable").unwrap();
        assert_eq!(read_file_exit_code(&profile, &recovery_file).unwrap(), 0);
        lease.abandon_for_recovery();
        assert!(journal_path.exists());
        drop(manager);
        let manager = WorkspaceLeaseManager::open(outside.path()).unwrap();
        assert!(!journal_path.exists());
        assert_eq!(manager.journal_count(), 0);
        assert_ne!(read_file_exit_code(&profile, &recovery_file).unwrap(), 0);

        let nonempty_workspace = tempfile::tempdir().unwrap();
        let nested = nonempty_workspace.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let existing_file = nested.join("existing.txt");
        std::fs::write(&existing_file, b"lease-readable").unwrap();
        let lease = manager
            .grant(
                nonempty_workspace.path(),
                profile.sid,
                WorkspaceAccess::ReadOnly,
            )
            .unwrap();
        assert_eq!(read_file_exit_code(&profile, &existing_file).unwrap(), 0);
        let renamed_file = nested.join("renamed.txt");
        std::fs::rename(&existing_file, &renamed_file).unwrap();
        lease.restore().unwrap();
        assert_ne!(read_file_exit_code(&profile, &renamed_file).unwrap(), 0);
    }

    #[test]
    fn legacy_root_journal_is_recovered_without_leaving_inherited_access() {
        let app_data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
        let original_dacl_sddl = read_dacl_sddl(workspace.path()).unwrap();
        grant_sid_access(
            workspace.path(),
            profile.sid,
            WorkspaceAccess::ReadOnly,
            true,
        )
        .unwrap();
        let inherited_file = workspace.path().join("legacy.txt");
        std::fs::write(&inherited_file, b"lease-readable").unwrap();
        assert_eq!(read_file_exit_code(&profile, &inherited_file).unwrap(), 0);
        let journal_path = manager.journal_root.join("legacy.json");
        write_lease_journal(
            &journal_path,
            &DirectoryLeaseJournal {
                version: 1,
                access: WorkspaceAccess::ReadOnly,
                workspace_path_utf16: workspace.path().as_os_str().encode_wide().collect(),
                original_dacl_sddl,
                sid_sddl: String::new(),
                entries: Vec::new(),
            },
        )
        .unwrap();
        drop(manager);

        let recovered = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        assert_eq!(recovered.journal_count(), 0);
        assert_ne!(read_file_exit_code(&profile, &inherited_file).unwrap(), 0);
    }

    #[test]
    fn workspace_tree_rejects_reparse_points_before_publishing_a_journal() {
        let app_data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("outside.txt");
        std::fs::write(&target, b"outside").unwrap();
        let link = workspace.path().join("link.txt");
        if std::os::windows::fs::symlink_file(&target, &link).is_err() {
            return;
        }
        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
        assert!(manager
            .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadOnly,)
            .is_err());
        assert_eq!(manager.journal_count(), 0);
    }

    #[test]
    fn workspace_tree_recovery_rejects_replaced_entry_identity() {
        let app_data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let existing_file = workspace.path().join("existing.txt");
        std::fs::write(&existing_file, b"original").unwrap();
        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
        let lease = manager
            .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadOnly)
            .unwrap();
        let journal_path = lease.journal_path.clone();
        lease.abandon_for_recovery();
        std::fs::remove_file(&existing_file).unwrap();
        std::fs::write(&existing_file, b"replacement").unwrap();
        drop(manager);

        let failure = WorkspaceLeaseManager::open(app_data.path())
            .err()
            .expect("recovery must reject a replaced entry");
        assert!(failure.contains("changed identity"), "{failure}");
        assert!(journal_path.exists());
        assert_eq!(std::fs::read(&existing_file).unwrap(), b"replacement");
    }

    #[test]
    fn read_write_lease_runs_real_git_against_an_existing_repository() {
        let located = std::process::Command::new("where.exe")
            .arg("git.exe")
            .output()
            .unwrap();
        if !located.status.success() {
            return;
        }
        let located_stdout = String::from_utf8_lossy(&located.stdout);
        let Some(git_line) = located_stdout.lines().find(|line| !line.trim().is_empty()) else {
            return;
        };
        let git = PathBuf::from(git_line.trim());
        let app_data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let initialized = std::process::Command::new(&git)
            .arg("init")
            .arg(workspace.path())
            .status()
            .unwrap();
        assert!(initialized.success());
        std::fs::write(workspace.path().join("fixture.txt"), b"fixture").unwrap();

        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
        let lease = manager
            .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadWrite)
            .unwrap();
        let command_line = build_windows_command_line(
            git.as_os_str(),
            &[
                "-C".to_owned(),
                workspace.path().to_string_lossy().into_owned(),
                "add".to_owned(),
                "--".to_owned(),
                "fixture.txt".to_owned(),
            ],
        )
        .unwrap();
        let captured = profile
            .run_captured(
                &git,
                &command_line,
                workspace.path(),
                Duration::from_secs(30),
                NetworkAccess::Disabled,
                None,
                1_024 * 1_024,
            )
            .unwrap();
        assert_eq!(
            captured.exit_code,
            0,
            "{}",
            String::from_utf8_lossy(&captured.stderr)
        );
        assert!(captured.stdout.is_empty());
        assert!(captured.capability_policy_verified);
        lease.restore().unwrap();

        let staged = std::process::Command::new(&git)
            .arg("-C")
            .arg(workspace.path())
            .args(["diff", "--cached", "--name-only"])
            .output()
            .unwrap();
        assert!(staged.status.success());
        assert_eq!(
            String::from_utf8_lossy(&staged.stdout).trim(),
            "fixture.txt"
        );
    }

    #[test]
    fn runtime_launcher_bounds_output_and_restores_its_workspace() {
        let system_root = std::env::var_os("SystemRoot").unwrap();
        let shell = Path::new(&system_root).join("System32").join("cmd.exe");
        let app_data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        let failure = manager
            .run_workspace_command(
                workspace.path(),
                &shell,
                &[
                    "/d".to_owned(),
                    "/c".to_owned(),
                    "for /L %i in (1,1,1000) do @echo 1234567890".to_owned(),
                ],
                WorkspaceAccess::ReadOnly,
                "disabled",
                &restricted_runtime_environment(),
                Duration::from_secs(10),
                1_024,
            )
            .unwrap_err();
        assert!(failure.contains("output exceeded"), "{failure}");
        assert_eq!(manager.journal_count(), 0);
    }

    #[test]
    fn interactive_runtime_round_trips_stdio_and_restores_its_leases() {
        let system_root = std::env::var_os("SystemRoot").unwrap();
        let shell = Path::new(&system_root).join("System32").join("cmd.exe");
        let app_data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        let mut child = manager
            .spawn_workspace_command(
                workspace.path(),
                &shell,
                &[
                    "/d".to_owned(),
                    "/q".to_owned(),
                    "/v:on".to_owned(),
                    "/c".to_owned(),
                    "set /p CHATOS_LINE=& echo reply:!CHATOS_LINE!".to_owned(),
                ],
                WorkspaceAccess::ReadOnly,
                "disabled",
                &restricted_runtime_environment(),
            )
            .unwrap();
        let mut stdin = child.take_stdin().unwrap();
        let mut stdout = child.take_stdout().unwrap();
        let mut stderr = child.take_stderr().unwrap();
        stdin.write_all(b"mcp-roundtrip\r\n").unwrap();
        drop(stdin);

        assert_eq!(child.wait(Duration::from_secs(10)).unwrap(), 0);
        let mut stdout_bytes = Vec::new();
        stdout.read_to_end(&mut stdout_bytes).unwrap();
        let mut stderr_bytes = Vec::new();
        stderr.read_to_end(&mut stderr_bytes).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&stdout_bytes).trim(),
            "reply:mcp-roundtrip"
        );
        assert!(
            stderr_bytes.is_empty(),
            "{}",
            String::from_utf8_lossy(&stderr_bytes)
        );
        assert_eq!(manager.journal_count(), 0);
        assert_eq!(manager.profile_journal_count(), 0);
    }

    #[test]
    fn interactive_runtime_rejects_overlapping_workspace_leases_before_launch() {
        let system_root = std::env::var_os("SystemRoot").unwrap();
        let shell = Path::new(&system_root).join("System32").join("cmd.exe");
        let app_data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let nested = workspace.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        let failure = manager
            .spawn_workspace_command_with_leases(
                workspace.path(),
                &shell,
                &["/d".to_owned(), "/c".to_owned(), "exit 0".to_owned()],
                &[
                    (workspace.path().to_owned(), WorkspaceAccess::ReadOnly),
                    (nested, WorkspaceAccess::ReadWrite),
                ],
                "disabled",
                &restricted_runtime_environment(),
            )
            .err()
            .expect("overlapping workspace leases must fail");
        assert!(failure.contains("overlap"), "{failure}");
        assert_eq!(manager.journal_count(), 0);
        assert_eq!(manager.profile_journal_count(), 0);
    }

    #[test]
    fn runtime_profile_journal_recovers_a_crash_abandoned_profile() {
        let app_data = tempfile::tempdir().unwrap();
        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        let profile = AppContainerProfile::create_journaled(&manager.profile_journal_root).unwrap();
        let profile_name = String::from_utf16(profile.name.strip_suffix(&[0]).unwrap()).unwrap();
        assert_eq!(manager.profile_journal_count(), 1);
        profile.abandon_for_recovery();

        assert_eq!(manager.recover_profiles().unwrap(), 1);
        assert_eq!(manager.profile_journal_count(), 0);
        let recreated = AppContainerProfile::create_named(&profile_name, None, None).unwrap();
        drop(recreated);
    }

    #[test]
    fn workspace_lease_startup_recovery_discards_only_unpublished_drafts() {
        let app_data = tempfile::tempdir().unwrap();
        let journal_root = app_data
            .path()
            .join("local-agent")
            .join("appcontainer-workspace-leases");
        std::fs::create_dir_all(&journal_root).unwrap();
        let token = "00000000-0000-0000-0000-000000000000";
        let legacy_draft = journal_root.join(format!("{token}.tmp"));
        let owned_draft =
            journal_root.join(format!("{WORKSPACE_JOURNAL_TEMPORARY_PREFIX}{token}.tmp"));
        std::fs::write(&legacy_draft, b"legacy-not-published").unwrap();
        std::fs::write(&owned_draft, b"not-published").unwrap();

        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        assert!(!legacy_draft.exists());
        assert!(!owned_draft.exists());
        assert_eq!(manager.journal_count(), 0);

        std::fs::write(
            journal_root.join(format!(
                "{WORKSPACE_JOURNAL_TEMPORARY_PREFIX}not-a-uuid.tmp"
            )),
            b"untrusted",
        )
        .unwrap();
        assert!(WorkspaceLeaseManager::open(app_data.path()).is_err());
    }

    #[test]
    fn profile_lease_startup_recovery_discards_only_owned_or_legacy_drafts() {
        let app_data = tempfile::tempdir().unwrap();
        let journal_root = app_data
            .path()
            .join("local-agent")
            .join("appcontainer-profile-leases");
        std::fs::create_dir_all(&journal_root).unwrap();
        let token = "00000000-0000-0000-0000-000000000000";
        let legacy_draft = journal_root.join(format!("{token}.tmp"));
        let owned_draft =
            journal_root.join(format!("{PROFILE_JOURNAL_TEMPORARY_PREFIX}{token}.tmp"));
        std::fs::write(&legacy_draft, b"legacy-not-published").unwrap();
        std::fs::write(&owned_draft, b"not-published").unwrap();

        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        assert!(!legacy_draft.exists());
        assert!(!owned_draft.exists());
        assert_eq!(manager.profile_journal_count(), 0);

        std::fs::write(
            journal_root.join(format!("{PROFILE_JOURNAL_TEMPORARY_PREFIX}not-a-uuid.tmp")),
            b"untrusted",
        )
        .unwrap();
        assert!(WorkspaceLeaseManager::open(app_data.path()).is_err());
    }
}
