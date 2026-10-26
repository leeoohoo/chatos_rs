use std::{
    ffi::{c_void, OsString},
    fs::OpenOptions,
    io::Write,
    mem::{size_of, zeroed},
    net::TcpListener,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{Mutex, MutexGuard, OnceLock},
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, LocalFree, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Security::{
        Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW,
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
            SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, GRANT_ACCESS,
            SDDL_REVISION_1, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP,
        },
        FreeSid,
        Isolation::{CreateAppContainerProfile, DeleteAppContainerProfile},
        SetFileSecurityW, ACL, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        SECURITY_CAPABILITIES, SUB_CONTAINERS_AND_OBJECTS_INHERIT,
    },
    Storage::FileSystem::{FILE_GENERIC_EXECUTE, FILE_GENERIC_READ},
    System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
        InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
        WaitForSingleObject, CREATE_NO_WINDOW, EXTENDED_STARTUPINFO_PRESENT,
        LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, STARTUPINFOEXW,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppContainerProbe {
    pub filesystem_denied: bool,
    pub network_denied: bool,
    pub workspace_lease_verified: bool,
    pub workspace_recovery_verified: bool,
}

static PROBE: OnceLock<Result<AppContainerProbe, String>> = OnceLock::new();
static PROFILE_LOCK: Mutex<()> = Mutex::new(());

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
    let exit_code = profile.run(&reader, &command_line, Duration::from_secs(10))?;
    let network_denied = probe_network_isolation(&profile, Path::new(&system_root))?;
    let workspace = tempfile::tempdir()
        .map_err(|error| format!("无法创建 AppContainer 工作区租约探测目录：{error}"))?;
    let journal_path = directory.path().join("workspace-lease.json");
    let lease = DirectoryReadLease::grant(workspace.path(), profile.sid, &journal_path)?;
    let workspace_file = workspace.path().join("workspace.txt");
    std::fs::write(&workspace_file, b"lease-readable")
        .map_err(|error| format!("无法写入 AppContainer 工作区租约探测文件：{error}"))?;
    let granted_exit = read_probe_file(&profile, &reader, &workspace_file)?;
    lease.restore()?;
    let restored_exit = read_probe_file(&profile, &reader, &workspace_file)?;
    let recovery_journal_path = directory.path().join("workspace-recovery-lease.json");
    let recovery_lease =
        DirectoryReadLease::grant(workspace.path(), profile.sid, &recovery_journal_path)?;
    let recovery_granted_exit = read_probe_file(&profile, &reader, &workspace_file)?;
    recovery_lease.abandon_for_recovery();
    DirectoryReadLease::recover(&recovery_journal_path)?;
    let recovered_exit = read_probe_file(&profile, &reader, &workspace_file)?;
    Ok(AppContainerProbe {
        filesystem_denied: exit_code != 0,
        network_denied,
        workspace_lease_verified: granted_exit == 0 && restored_exit != 0 && !journal_path.exists(),
        workspace_recovery_verified: recovery_granted_exit == 0
            && recovered_exit != 0
            && !recovery_journal_path.exists(),
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
            .map_err(|_| "Windows AppContainer profile lock is poisoned.".to_owned())?;
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
        let executable = wide_os(executable.as_os_str());
        let mut command_line = wide(command_line);
        let mut attribute_list = AttributeList::new()?;
        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: self.sid,
            Capabilities: null_mut(),
            CapabilityCount: 0,
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
                EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
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
        Ok(exit_code)
    }
}

struct DirectoryReadLease {
    path: Vec<u16>,
    journal_path: PathBuf,
    original_descriptor: PSECURITY_DESCRIPTOR,
    original_dacl: *mut ACL,
    restored: bool,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct DirectoryReadLeaseJournal {
    version: u32,
    workspace_path_utf16: Vec<u16>,
    original_dacl_sddl: String,
}

impl DirectoryReadLease {
    fn grant(path: &Path, sid: PSID, journal_path: &Path) -> Result<Self, String> {
        if journal_path.exists() {
            Self::recover(journal_path)?;
        }
        let workspace_path_utf16: Vec<u16> = path.as_os_str().encode_wide().collect();
        let path = wide_os(path.as_os_str());
        let mut original_dacl = null_mut();
        let mut original_descriptor = null_mut();
        let read_result = unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut original_dacl,
                null_mut(),
                &mut original_descriptor,
            )
        };
        if read_result != 0 || original_descriptor.is_null() {
            return Err(format!(
                "Reading the workspace ACL failed (Win32 error {read_result})."
            ));
        }
        let original_dacl_sddl = match descriptor_to_sddl(original_descriptor) {
            Ok(sddl) => sddl,
            Err(error) => {
                unsafe {
                    LocalFree(original_descriptor.cast());
                }
                return Err(error);
            }
        };
        let journal = DirectoryReadLeaseJournal {
            version: 1,
            workspace_path_utf16,
            original_dacl_sddl,
        };
        if let Err(error) = write_lease_journal(journal_path, &journal) {
            unsafe {
                LocalFree(original_descriptor.cast());
            }
            return Err(error);
        }

        let mut entry: EXPLICIT_ACCESS_W = unsafe { zeroed() };
        entry.grfAccessPermissions = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
        entry.grfAccessMode = GRANT_ACCESS;
        entry.grfInheritance = SUB_CONTAINERS_AND_OBJECTS_INHERIT;
        entry.Trustee.TrusteeForm = TRUSTEE_IS_SID;
        entry.Trustee.TrusteeType = TRUSTEE_IS_WELL_KNOWN_GROUP;
        entry.Trustee.ptstrName = sid.cast();
        let mut updated_dacl = null_mut();
        let update_result =
            unsafe { SetEntriesInAclW(1, &entry, original_dacl, &mut updated_dacl) };
        if update_result != 0 || updated_dacl.is_null() {
            unsafe {
                LocalFree(original_descriptor.cast());
            }
            let _ = std::fs::remove_file(journal_path);
            return Err(format!(
                "Building the workspace ACL lease failed (Win32 error {update_result})."
            ));
        }
        let apply_result = unsafe {
            SetNamedSecurityInfoW(
                path.as_ptr() as *mut u16,
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
        }
        if apply_result != 0 {
            unsafe {
                LocalFree(original_descriptor.cast());
            }
            let _ = std::fs::remove_file(journal_path);
            return Err(format!(
                "Applying the workspace ACL lease failed (Win32 error {apply_result})."
            ));
        }
        Ok(Self {
            path,
            journal_path: journal_path.to_path_buf(),
            original_descriptor,
            original_dacl,
            restored: false,
        })
    }

    fn recover(journal_path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(journal_path)
            .map_err(|error| format!("Reading the workspace ACL journal failed: {error}"))?;
        let journal: DirectoryReadLeaseJournal = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Parsing the workspace ACL journal failed: {error}"))?;
        if journal.version != 1
            || journal.workspace_path_utf16.is_empty()
            || journal.workspace_path_utf16.contains(&0)
        {
            return Err("The workspace ACL journal is invalid.".to_owned());
        }
        let workspace = PathBuf::from(OsString::from_wide(&journal.workspace_path_utf16));
        restore_dacl_from_sddl(&workspace, &journal.original_dacl_sddl)?;
        std::fs::remove_file(journal_path)
            .map_err(|error| format!("Removing the restored workspace ACL journal failed: {error}"))
    }

    fn restore(mut self) -> Result<(), String> {
        self.restore_inner()
    }

    fn abandon_for_recovery(mut self) {
        unsafe {
            LocalFree(self.original_descriptor.cast());
        }
        self.original_descriptor = null_mut();
        self.original_dacl = null_mut();
        self.restored = true;
    }

    fn restore_inner(&mut self) -> Result<(), String> {
        if self.restored {
            return Ok(());
        }
        let result = unsafe {
            SetNamedSecurityInfoW(
                self.path.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                self.original_dacl,
                null(),
            )
        };
        if result != 0 {
            return Err(format!(
                "Restoring the workspace ACL failed (Win32 error {result})."
            ));
        }
        unsafe {
            LocalFree(self.original_descriptor.cast());
        }
        self.original_descriptor = null_mut();
        self.original_dacl = null_mut();
        self.restored = true;
        std::fs::remove_file(&self.journal_path)
            .map_err(|error| format!("Removing the restored workspace ACL journal failed: {error}"))
    }
}

impl Drop for DirectoryReadLease {
    fn drop(&mut self) {
        let _ = self.restore_inner();
        if !self.original_descriptor.is_null() {
            unsafe {
                LocalFree(self.original_descriptor.cast());
            }
            self.original_descriptor = null_mut();
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

fn write_lease_journal(
    journal_path: &Path,
    journal: &DirectoryReadLeaseJournal,
) -> Result<(), String> {
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
    }

    #[test]
    fn directory_read_lease_grants_only_the_workspace_and_restores_denial() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("outside.txt");
        std::fs::write(&outside_file, b"lease-readable").unwrap();
        let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
        let journal_path = outside.path().join("workspace-lease.json");
        let lease =
            DirectoryReadLease::grant(workspace.path(), profile.sid, &journal_path).unwrap();
        let workspace_file = workspace.path().join("workspace.txt");
        std::fs::write(&workspace_file, b"lease-readable").unwrap();

        assert_eq!(read_file_exit_code(&profile, &workspace_file).unwrap(), 0);
        assert_ne!(read_file_exit_code(&profile, &outside_file).unwrap(), 0);

        lease.restore().unwrap();
        assert!(!journal_path.exists());
        assert_ne!(read_file_exit_code(&profile, &workspace_file).unwrap(), 0);

        let lease =
            DirectoryReadLease::grant(workspace.path(), profile.sid, &journal_path).unwrap();
        assert_eq!(read_file_exit_code(&profile, &workspace_file).unwrap(), 0);
        lease.abandon_for_recovery();
        assert!(journal_path.exists());
        DirectoryReadLease::recover(&journal_path).unwrap();
        assert!(!journal_path.exists());
        assert_ne!(read_file_exit_code(&profile, &workspace_file).unwrap(), 0);
    }
}
