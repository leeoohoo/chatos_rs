use std::{
    collections::HashSet,
    ffi::{c_void, OsString},
    fs::OpenOptions,
    io::Write,
    mem::{size_of, zeroed},
    net::TcpListener,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{Arc, Mutex, MutexGuard, OnceLock},
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, LocalFree, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Security::{
        Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
            ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
            GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W,
            GRANT_ACCESS, REVOKE_ACCESS, SDDL_REVISION_1, SE_FILE_OBJECT, TRUSTEE_IS_SID,
            TRUSTEE_IS_WELL_KNOWN_GROUP,
        },
        CreateWellKnownSid, FreeSid, GetTokenInformation,
        Isolation::{CreateAppContainerProfile, DeleteAppContainerProfile},
        SetFileSecurityW, TokenCapabilities, WinCapabilityInternetClientSid,
        DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_CAPABILITIES,
        SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES, SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        TOKEN_GROUPS, TOKEN_QUERY,
    },
    Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE,
    },
    System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
        InitializeProcThreadAttributeList, OpenProcessToken, ResumeThread, TerminateProcess,
        UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, STARTUPINFOEXW,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppContainerProbe {
    pub filesystem_denied: bool,
    pub network_denied: bool,
    pub workspace_lease_verified: bool,
    pub workspace_recovery_verified: bool,
    pub workspace_permission_tiers_verified: bool,
    pub nonempty_workspace_tree_verified: bool,
    pub network_capability_policy_verified: bool,
}

static PROBE: OnceLock<Result<AppContainerProbe, String>> = OnceLock::new();
static PROFILE_LOCK: Mutex<()> = Mutex::new(());
const MAXIMUM_LEASE_TREE_ENTRIES: usize = 100_000;
const MAXIMUM_LEASE_JOURNAL_BYTES: usize = 64 * 1_024 * 1_024;

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkspaceAccess {
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

pub(crate) struct WorkspaceLeaseManager {
    journal_root: PathBuf,
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
        let manager = Self {
            journal_root,
            active_workspaces: Arc::new(Mutex::new(HashSet::new())),
        };
        manager.recover_all()?;
        Ok(manager)
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
            match path.extension().and_then(|value| value.to_str()) {
                Some("tmp") => std::fs::remove_file(&path).map_err(|error| {
                    format!("Removing an incomplete workspace lease journal failed: {error}")
                })?,
                Some("json") => {
                    DirectoryLease::recover(&path)?;
                    recovered += 1;
                }
                _ => {
                    return Err(
                        "The AppContainer workspace lease directory contains an unknown record."
                            .to_owned(),
                    )
                }
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
        network_capability_policy_verified: disabled_network_process.capability_policy_verified
            && controlled_network_process.capability_policy_verified,
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
    _guard: MutexGuard<'static, ()>,
    name: Vec<u16>,
    sid: PSID,
}

impl AppContainerProfile {
    fn create(name: &str) -> Result<Self, String> {
        let guard = PROFILE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let name = wide(name);
        let display_name = wide("ChatOS AppContainer Probe");
        let description = wide("Temporary ChatOS Windows isolation capability probe");
        unsafe {
            // A process termination can bypass Rust destructors. The deterministic probe
            // identity lets the next run remove a stale profile before recreating it.
            DeleteAppContainerProfile(name.as_ptr());
        }
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
            _guard: guard,
            name,
            sid,
        })
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
    restored: bool,
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
        let tree = lease_tree_paths(path)?;
        let mut entries = Vec::with_capacity(tree.len());
        for (entry_path, is_directory) in &tree {
            let relative = entry_path
                .strip_prefix(path)
                .map_err(|_| "The workspace ACL lease tree escaped its root.".to_owned())?;
            entries.push(DirectoryLeaseJournalEntry {
                relative_path_utf16: relative.as_os_str().encode_wide().collect(),
                original_dacl_sddl: read_dacl_sddl(entry_path)?,
                is_directory: *is_directory,
            });
        }
        let journal = DirectoryLeaseJournal {
            version: 2,
            access,
            workspace_path_utf16: path.as_os_str().encode_wide().collect(),
            original_dacl_sddl: String::new(),
            sid_sddl: sid_to_sddl(sid)?,
            entries,
        };
        write_lease_journal(journal_path, &journal)?;

        for (entry_path, is_directory) in &tree {
            if let Err(failure) = grant_sid_access(entry_path, sid, access, *is_directory) {
                return match restore_lease_journal(journal_path) {
                    Ok(()) => Err(failure),
                    Err(restore_failure) => Err(format!(
                        "{failure} Restoring the partial workspace ACL lease also failed: {restore_failure}"
                    )),
                };
            }
        }
        Ok(Self {
            workspace: path.to_owned(),
            journal_path: journal_path.to_owned(),
            active_workspaces,
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
        restore_lease_journal(&self.journal_path)?;
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
        2 => restore_tree_lease(&journal)?,
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
    let current_before_restore = lease_tree_paths(&workspace)?;
    let sid = LocalSid::from_sddl(&journal.sid_sddl)?;
    let mut original_paths = HashSet::with_capacity(journal.entries.len());
    let mut restored_paths = Vec::with_capacity(journal.entries.len());
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
        restored_paths.push((
            workspace.join(relative),
            entry.is_directory,
            entry.original_dacl_sddl.as_str(),
        ));
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

    restored_paths.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| {
                left.0
                    .components()
                    .count()
                    .cmp(&right.0.components().count())
            })
            .then_with(|| left.0.cmp(&right.0))
    });
    for (path, _, sddl) in restored_paths {
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if is_reparse_point(&metadata) {
                    return Err(
                        "A reparse point appeared inside the workspace ACL lease.".to_owned()
                    );
                }
                restore_dacl_from_sddl(&path, sddl)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Inspecting a workspace ACL recovery entry failed: {error}"
                ))
            }
        }
    }

    for (path, _) in current_before_restore {
        let relative = path
            .strip_prefix(&workspace)
            .map_err(|_| "The workspace ACL recovery tree escaped its root.".to_owned())?;
        if !original_paths.contains(relative) {
            revoke_sid_access(&path, sid.0)?;
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

fn lease_tree_paths(root: &Path) -> Result<Vec<(PathBuf, bool)>, String> {
    let metadata = std::fs::symlink_metadata(root)
        .map_err(|error| format!("Inspecting the workspace ACL tree failed: {error}"))?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err("The workspace ACL tree root is invalid.".to_owned());
    }
    let mut result = vec![(root.to_owned(), true)];
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let mut children = std::fs::read_dir(&directory)
            .map_err(|error| format!("Reading the workspace ACL tree failed: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Reading the workspace ACL tree failed: {error}"))?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("Inspecting the workspace ACL tree failed: {error}"))?;
            if is_reparse_point(&metadata) {
                return Err("The workspace ACL tree contains a reparse point.".to_owned());
            }
            let is_directory = metadata.is_dir();
            if !is_directory && !metadata.is_file() {
                return Err("The workspace ACL tree contains an unsupported entry.".to_owned());
            }
            result.push((path.clone(), is_directory));
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
            .1
            .cmp(&left.1)
            .then_with(|| {
                left.0
                    .components()
                    .count()
                    .cmp(&right.0.components().count())
            })
            .then_with(|| left.0.cmp(&right.0))
    });
    Ok(result)
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

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

fn grant_sid_access(
    path: &Path,
    sid: PSID,
    access: WorkspaceAccess,
    is_directory: bool,
) -> Result<(), String> {
    change_sid_access(path, sid, Some(access), is_directory, GRANT_ACCESS)
}

fn revoke_sid_access(path: &Path, sid: PSID) -> Result<(), String> {
    let is_directory = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata.is_dir(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Inspecting a new workspace ACL entry failed: {error}"
            ))
        }
    };
    change_sid_access(path, sid, None, is_directory, REVOKE_ACCESS)
}

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

fn write_lease_journal(journal_path: &Path, journal: &DirectoryLeaseJournal) -> Result<(), String> {
    let parent = journal_path
        .parent()
        .ok_or_else(|| "The workspace ACL journal has no parent directory.".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Creating the workspace ACL journal directory failed: {error}"))?;
    let temporary_path = journal_path.with_extension("tmp");
    if temporary_path.exists() {
        std::fs::remove_file(&temporary_path).map_err(|error| {
            format!("Removing a stale workspace ACL journal draft failed: {error}")
        })?;
    }
    let payload = serde_json::to_vec(journal)
        .map_err(|error| format!("Encoding the workspace ACL journal failed: {error}"))?;
    if payload.len() > MAXIMUM_LEASE_JOURNAL_BYTES {
        return Err("The workspace ACL journal exceeds its safety limit.".to_owned());
    }
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .map_err(|error| format!("Creating the workspace ACL journal failed: {error}"))?;
        file.write_all(&payload)
            .map_err(|error| format!("Writing the workspace ACL journal failed: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Flushing the workspace ACL journal failed: {error}"))?;
        drop(file);
        std::fs::rename(&temporary_path, journal_path)
            .map_err(|error| format!("Publishing the workspace ACL journal failed: {error}"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result
}

impl Drop for AppContainerProfile {
    fn drop(&mut self) {
        unsafe {
            if !self.sid.is_null() {
                FreeSid(self.sid);
            }
            DeleteAppContainerProfile(self.name.as_ptr());
        }
    }
}

struct AttributeList {
    _storage: Vec<u8>,
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn new() -> Result<Self, String> {
        let mut bytes = 0usize;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(last_error(
                "Sizing the Windows process attribute list failed",
            ));
        }
        let mut storage = vec![0u8; bytes];
        let pointer = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(pointer, 1, 0, &mut bytes) } == 0 {
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

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                CloseHandle(self.0);
            }
        }
    }
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

fn wide_os(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn last_error(context: &str) -> String {
    let code = unsafe { GetLastError() };
    format!("{context} (Win32 error {code}).")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        lease.restore().unwrap();
        assert_ne!(read_file_exit_code(&profile, &existing_file).unwrap(), 0);
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
    fn workspace_lease_startup_recovery_discards_only_unpublished_drafts() {
        let app_data = tempfile::tempdir().unwrap();
        let journal_root = app_data
            .path()
            .join("local-agent")
            .join("appcontainer-workspace-leases");
        std::fs::create_dir_all(&journal_root).unwrap();
        let draft = journal_root.join("interrupted.tmp");
        std::fs::write(&draft, b"not-published").unwrap();

        let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
        assert!(!draft.exists());
        assert_eq!(manager.journal_count(), 0);

        std::fs::write(journal_root.join("unexpected.txt"), b"untrusted").unwrap();
        assert!(WorkspaceLeaseManager::open(app_data.path()).is_err());
    }
}
