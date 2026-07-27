// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{c_void, OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetHandleInformation, LocalFree, SetHandleInformation, HANDLE,
    HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    BuildTrusteeWithSidW, ConvertSidToStringSidW, GetNamedSecurityInfoW, SetEntriesInAclW,
    SetNamedSecurityInfoW, DENY_ACCESS, EXPLICIT_ACCESS_W, GRANT_ACCESS, SE_FILE_OBJECT,
    TRUSTEE_IS_UNKNOWN,
};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile, GetAppContainerFolderPath,
};
use windows_sys::Win32::Security::{
    FreeSid, DACL_SECURITY_INFORMATION, NO_INHERITANCE, PSID, SECURITY_CAPABILITIES,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_APPEND_DATA, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD,
    FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA,
    WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicUIRestrictions,
    JobObjectExtendedLimitInformation, SetInformationJobObject, JOBOBJECT_BASIC_UI_RESTRICTIONS,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_UILIMIT_DESKTOP,
    JOB_OBJECT_UILIMIT_DISPLAYSETTINGS, JOB_OBJECT_UILIMIT_EXITWINDOWS,
    JOB_OBJECT_UILIMIT_GLOBALATOMS, JOB_OBJECT_UILIMIT_HANDLES, JOB_OBJECT_UILIMIT_READCLIPBOARD,
    JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, ResumeThread, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    EXTENDED_STARTUPINFO_PRESENT, INFINITE, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

use super::{windows_command_line, PluginStdioSandboxSpec};

const PACKAGE_INDEX_MAX_BYTES: u64 = 2 * 1024 * 1024;
const PACKAGE_FILE_MAX_BYTES: u64 = 256 * 1024 * 1024;
const PACKAGE_TOTAL_MAX_BYTES: u64 = 512 * 1024 * 1024;
const PACKAGE_MAX_FILES: usize = 8_192;
const JOB_ACTIVE_PROCESS_LIMIT: u32 = 32;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedPackageIndex {
    schema_version: u32,
    files: BTreeMap<String, String>,
}

pub(super) async fn run(spec: &PluginStdioSandboxSpec) -> Result<i32, String> {
    let spec = spec.clone();
    tokio::task::spawn_blocking(move || run_blocking(&spec))
        .await
        .map_err(|_| "Windows Plugin stdio sandbox worker failed".to_string())?
}

fn run_blocking(spec: &PluginStdioSandboxSpec) -> Result<i32, String> {
    if spec.workspace_root.is_some() {
        return Err(
            "Windows Plugin stdio AppContainer does not yet support workspace-write".to_string(),
        );
    }

    let profile = AppContainerProfile::create(spec.sandbox_id.as_str())?;
    let profile_root = profile.folder_path()?;
    let staged = stage_signed_package(spec, profile_root.as_path(), profile.sid())?;
    let environment = safe_environment_block(spec, &staged)?;
    launch_appcontainer_process(spec, &profile, &staged, environment.as_slice())
}

struct StagedPackage {
    sandbox_root: PathBuf,
    plugin_root: PathBuf,
    state_root: PathBuf,
    cache_root: PathBuf,
    temp_root: PathBuf,
    cwd: PathBuf,
    command: PathBuf,
}

fn stage_signed_package(
    spec: &PluginStdioSandboxSpec,
    profile_root: &Path,
    appcontainer_sid: PSID,
) -> Result<StagedPackage, String> {
    let index_metadata = fs::symlink_metadata(spec.package_index.as_path())
        .map_err(|error| format!("read Plugin stdio package index metadata failed: {error}"))?;
    if !index_metadata.is_file()
        || index_metadata.file_type().is_symlink()
        || index_metadata.len() > PACKAGE_INDEX_MAX_BYTES
    {
        return Err("Plugin stdio signed package index is unsafe or too large".to_string());
    }
    let index_bytes = fs::read(spec.package_index.as_path())
        .map_err(|error| format!("read Plugin stdio signed package index failed: {error}"))?;
    if index_bytes.len() as u64 > PACKAGE_INDEX_MAX_BYTES {
        return Err("Plugin stdio signed package index is too large".to_string());
    }
    let index: SignedPackageIndex = serde_json::from_slice(index_bytes.as_slice())
        .map_err(|error| format!("parse Plugin stdio signed package index failed: {error}"))?;
    if index.schema_version != 1 || index.files.is_empty() || index.files.len() > PACKAGE_MAX_FILES
    {
        return Err("Plugin stdio signed package index has invalid bounds".to_string());
    }

    let command_relative =
        package_relative_path(spec.command.as_path(), spec.plugin_root.as_path())?;
    let cwd_relative = package_relative_path(spec.cwd.as_path(), spec.plugin_root.as_path())?;
    let command_key = package_key(command_relative.as_path())?;
    if !index.files.contains_key(command_key.as_str()) {
        return Err("Plugin stdio command is absent from the signed package index".to_string());
    }

    let local_state_root = profile_root.join("LocalState");
    let guard_root = local_state_root.join("ChatOS");
    let sandbox_root = guard_root.join(spec.sandbox_id.as_str());
    let plugin_root = sandbox_root.join("plugin");
    let state_root = sandbox_root.join("state");
    let cache_root = sandbox_root.join("cache");
    let temp_root = sandbox_root.join("tmp");
    for path in [
        sandbox_root.as_path(),
        plugin_root.as_path(),
        state_root.as_path(),
        cache_root.as_path(),
        temp_root.as_path(),
        state_root.join("local-app-data").as_path(),
        state_root.join("roaming-app-data").as_path(),
    ] {
        fs::create_dir_all(path)
            .map_err(|error| format!("create Windows Plugin sandbox directory failed: {error}"))?;
    }

    let mut total_bytes = 0_u64;
    let mut staged_paths = BTreeSet::from([plugin_root.clone()]);
    for (relative, expected_sha256) in &index.files {
        validate_sha256(expected_sha256.as_str())?;
        let relative_path = validate_package_index_path(relative.as_str())?;
        let source = spec.plugin_root.join(relative_path.as_path());
        let destination = plugin_root.join(relative_path.as_path());
        validate_source_ancestors(
            source.as_path(),
            relative_path.as_path(),
            spec.plugin_root.as_path(),
        )?;
        let parent = destination
            .parent()
            .ok_or_else(|| "Plugin stdio staged package file has no parent".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("create Windows Plugin staged directory failed: {error}"))?;
        collect_ancestors(parent, plugin_root.as_path(), &mut staged_paths)?;
        copy_verified_file(
            source.as_path(),
            destination.as_path(),
            expected_sha256.as_str(),
            &mut total_bytes,
        )?;
        staged_paths.insert(destination);
    }

    let cwd = plugin_root.join(cwd_relative.as_path());
    fs::create_dir_all(cwd.as_path())
        .map_err(|error| format!("create Windows Plugin staged cwd failed: {error}"))?;
    collect_ancestors(cwd.as_path(), plugin_root.as_path(), &mut staged_paths)?;
    let command = plugin_root.join(command_relative.as_path());
    if !command.is_file() {
        return Err("Windows Plugin staged command is unavailable".to_string());
    }

    // The profile grants its AppContainer write access. Exact deny ACEs on the sandbox root stop
    // entry replacement, and exact deny ACEs on every staged package object keep signed content
    // read/execute-only while state/cache/tmp retain their inherited write access.
    grant_appcontainer_read_only(profile_root, appcontainer_sid)?;
    grant_appcontainer_read_only(local_state_root.as_path(), appcontainer_sid)?;
    grant_appcontainer_read_only(guard_root.as_path(), appcontainer_sid)?;
    grant_appcontainer_read_only(sandbox_root.as_path(), appcontainer_sid)?;
    for path in staged_paths {
        grant_appcontainer_read_only(path.as_path(), appcontainer_sid)?;
    }

    Ok(StagedPackage {
        sandbox_root,
        plugin_root,
        state_root,
        cache_root,
        temp_root,
        cwd,
        command,
    })
}

fn package_relative_path(path: &Path, root: &Path) -> Result<PathBuf, String> {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|_| "Plugin stdio path escaped Plugin root".to_string())
}

fn package_key(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(component) = component else {
            return Err("Plugin stdio package path is not normalized".to_string());
        };
        let component = component
            .to_str()
            .ok_or_else(|| "Plugin stdio package path is not Unicode".to_string())?;
        parts.push(component);
    }
    if parts.is_empty() {
        return Err("Plugin stdio package path is empty".to_string());
    }
    Ok(parts.join("/"))
}

fn validate_package_index_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty()
        || value.len() > 1024
        || value.contains('\\')
        || value.contains(':')
        || value.contains('\0')
        || value.starts_with('/')
    {
        return Err("Plugin stdio signed package path is invalid".to_string());
    }
    let mut path = PathBuf::new();
    for component in value.split('/') {
        if component.is_empty() || matches!(component, "." | "..") {
            return Err("Plugin stdio signed package path is not normalized".to_string());
        }
        validate_windows_path_component(component)?;
        path.push(component);
    }
    Ok(path)
}

fn validate_windows_path_component(value: &str) -> Result<(), String> {
    if value.ends_with([' ', '.'])
        || value.chars().any(|character| {
            character <= '\u{1f}' || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        })
    {
        return Err("Plugin stdio signed package path is not Windows-safe".to_string());
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            });
    if reserved {
        return Err("Plugin stdio signed package path uses a Windows device name".to_string());
    }
    Ok(())
}

fn validate_source_ancestors(source: &Path, relative: &Path, root: &Path) -> Result<(), String> {
    let mut current = root.to_path_buf();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let std::path::Component::Normal(component) = component else {
            return Err("Plugin stdio source path is not normalized".to_string());
        };
        current.push(component);
        if components.peek().is_none() {
            break;
        }
        let metadata = fs::symlink_metadata(current.as_path()).map_err(|error| {
            format!("read signed Plugin package directory metadata failed: {error}")
        })?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err("signed Plugin package directory is unsafe".to_string());
        }
    }
    if current != source {
        return Err("Plugin stdio source path changed during validation".to_string());
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("Plugin stdio signed package SHA-256 is invalid".to_string());
    }
    Ok(())
}

fn collect_ancestors(
    path: &Path,
    root: &Path,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let mut current = path.to_path_buf();
    loop {
        if !current.starts_with(root) {
            return Err("Plugin stdio staged path escaped its root".to_string());
        }
        paths.insert(current.clone());
        if current == root {
            return Ok(());
        }
        current = current
            .parent()
            .ok_or_else(|| "Plugin stdio staged path has no parent".to_string())?
            .to_path_buf();
    }
}

fn copy_verified_file(
    source: &Path,
    destination: &Path,
    expected_sha256: &str,
    total_bytes: &mut u64,
) -> Result<(), String> {
    let before = fs::symlink_metadata(source)
        .map_err(|error| format!("read signed Plugin package file metadata failed: {error}"))?;
    if !before.is_file()
        || before.file_type().is_symlink()
        || before.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || before.len() > PACKAGE_FILE_MAX_BYTES
    {
        return Err("signed Plugin package file is unsafe or too large".to_string());
    }
    *total_bytes = total_bytes
        .checked_add(before.len())
        .filter(|total| *total <= PACKAGE_TOTAL_MAX_BYTES)
        .ok_or_else(|| "signed Plugin package exceeds Windows staging limit".to_string())?;

    let mut input = File::open(source)
        .map_err(|error| format!("open signed Plugin package file failed: {error}"))?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| format!("create staged Plugin package file failed: {error}"))?;
    let mut hasher = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("read signed Plugin package file failed: {error}"))?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(read as u64)
            .filter(|size| *size <= PACKAGE_FILE_MAX_BYTES)
            .ok_or_else(|| "signed Plugin package file grew beyond its limit".to_string())?;
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|error| format!("write staged Plugin package file failed: {error}"))?;
    }
    output
        .sync_all()
        .map_err(|error| format!("sync staged Plugin package file failed: {error}"))?;
    let after = fs::symlink_metadata(source)
        .map_err(|error| format!("re-read signed Plugin package metadata failed: {error}"))?;
    if copied != before.len()
        || after.len() != before.len()
        || after.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err("signed Plugin package file changed during staging".to_string());
    }
    let actual_sha256 = hex::encode(hasher.finalize());
    if actual_sha256 != expected_sha256 {
        return Err("signed Plugin package file failed SHA-256 verification".to_string());
    }
    Ok(())
}

fn grant_appcontainer_read_only(path: &Path, sid: PSID) -> Result<(), String> {
    let path_wide = wide_nul(path.as_os_str())?;
    let mut old_dacl = null_mut();
    let mut security_descriptor = null_mut();
    let mut new_dacl = null_mut();
    let result = unsafe {
        let status = GetNamedSecurityInfoW(
            path_wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut old_dacl,
            null_mut(),
            &mut security_descriptor,
        );
        if status != 0 {
            Err(format!(
                "read Windows Plugin sandbox ACL failed with Win32 error {status}"
            ))
        } else {
            // Do not deny FILE_GENERIC_WRITE as a single mask: it contains READ_CONTROL and would
            // also block FILE_GENERIC_READ/EXECUTE. Deny only the concrete mutation rights.
            let denied = FILE_WRITE_DATA
                | FILE_APPEND_DATA
                | FILE_WRITE_EA
                | FILE_WRITE_ATTRIBUTES
                | DELETE
                | FILE_DELETE_CHILD
                | WRITE_DAC
                | WRITE_OWNER;
            let allowed = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
            let mut entries = [EXPLICIT_ACCESS_W::default(), EXPLICIT_ACCESS_W::default()];
            entries[0].grfAccessPermissions = denied;
            entries[0].grfAccessMode = DENY_ACCESS;
            entries[0].grfInheritance = NO_INHERITANCE;
            BuildTrusteeWithSidW(&mut entries[0].Trustee, sid);
            entries[0].Trustee.TrusteeType = TRUSTEE_IS_UNKNOWN;
            entries[1].grfAccessPermissions = allowed;
            entries[1].grfAccessMode = GRANT_ACCESS;
            entries[1].grfInheritance = NO_INHERITANCE;
            BuildTrusteeWithSidW(&mut entries[1].Trustee, sid);
            entries[1].Trustee.TrusteeType = TRUSTEE_IS_UNKNOWN;
            let status = SetEntriesInAclW(
                entries.len() as u32,
                entries.as_ptr(),
                old_dacl,
                &mut new_dacl,
            );
            if status != 0 {
                Err(format!(
                    "build Windows Plugin sandbox ACL failed with Win32 error {status}"
                ))
            } else {
                let status = SetNamedSecurityInfoW(
                    path_wide.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    new_dacl,
                    null_mut(),
                );
                if status == 0 {
                    Ok(())
                } else {
                    Err(format!(
                        "apply Windows Plugin sandbox ACL failed with Win32 error {status}"
                    ))
                }
            }
        }
    };
    unsafe {
        if !new_dacl.is_null() {
            LocalFree(new_dacl.cast());
        }
        if !security_descriptor.is_null() {
            LocalFree(security_descriptor);
        }
    }
    result
}

fn safe_environment_block(
    spec: &PluginStdioSandboxSpec,
    staged: &StagedPackage,
) -> Result<Vec<u16>, String> {
    let system_root = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("WINDIR"))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Windows Plugin stdio SystemRoot is unavailable".to_string())?;
    let mut values = BTreeMap::<String, (String, OsString)>::new();
    insert_environment(&mut values, "SystemRoot", system_root.clone())?;
    insert_environment(&mut values, "WINDIR", system_root.clone())?;
    let mut path = system_root.clone();
    path.push("\\System32;");
    path.push(system_root.as_os_str());
    insert_environment(&mut values, "PATH", path)?;
    insert_environment(
        &mut values,
        "USERPROFILE",
        staged.sandbox_root.as_os_str().to_os_string(),
    )?;
    insert_environment(
        &mut values,
        "LOCALAPPDATA",
        staged.state_root.join("local-app-data").into_os_string(),
    )?;
    insert_environment(
        &mut values,
        "APPDATA",
        staged.state_root.join("roaming-app-data").into_os_string(),
    )?;
    insert_environment(
        &mut values,
        "HOME",
        staged.state_root.as_os_str().to_os_string(),
    )?;
    insert_environment(
        &mut values,
        "TEMP",
        staged.temp_root.as_os_str().to_os_string(),
    )?;
    insert_environment(
        &mut values,
        "TMP",
        staged.temp_root.as_os_str().to_os_string(),
    )?;
    insert_environment(
        &mut values,
        "XDG_CACHE_HOME",
        staged.cache_root.as_os_str().to_os_string(),
    )?;
    insert_environment(
        &mut values,
        "XDG_CONFIG_HOME",
        staged.state_root.join("config").into_os_string(),
    )?;
    insert_environment(
        &mut values,
        "XDG_STATE_HOME",
        staged.state_root.join("state").into_os_string(),
    )?;
    insert_environment(
        &mut values,
        "CHATOS_PLUGIN_ROOT",
        staged.plugin_root.as_os_str().to_os_string(),
    )?;
    for name in ["LANG", "LC_ALL", "LC_CTYPE", "LOGNAME", "TERM", "USER"] {
        if let Some(value) = std::env::var_os(name).filter(|value| !value.is_empty()) {
            insert_environment(&mut values, name, value)?;
        }
    }
    for name in &spec.environment_names {
        let value = std::env::var_os(name)
            .ok_or_else(|| format!("Plugin stdio environment value is unavailable: {name}"))?;
        insert_environment(&mut values, name.as_str(), value)?;
    }

    let mut block = Vec::new();
    for (_, (name, value)) in values {
        block.extend(name.encode_utf16());
        block.push(b'=' as u16);
        let encoded = value.encode_wide().collect::<Vec<_>>();
        if encoded.contains(&0) {
            return Err("Windows Plugin stdio environment value contains NUL".to_string());
        }
        block.extend(encoded);
        block.push(0);
    }
    block.push(0);
    if block.len() > 32_767 {
        return Err("Windows Plugin stdio environment block is too large".to_string());
    }
    Ok(block)
}

fn insert_environment(
    values: &mut BTreeMap<String, (String, OsString)>,
    name: &str,
    value: OsString,
) -> Result<(), String> {
    if name.is_empty() || name.contains('=') || name.contains('\0') {
        return Err("Windows Plugin stdio environment name is invalid".to_string());
    }
    values.insert(name.to_ascii_uppercase(), (name.to_string(), value));
    Ok(())
}

fn launch_appcontainer_process(
    spec: &PluginStdioSandboxSpec,
    profile: &AppContainerProfile,
    staged: &StagedPackage,
    environment: &[u16],
) -> Result<i32, String> {
    let application = wide_nul(staged.command.as_os_str())?;
    let cwd = wide_nul(staged.cwd.as_os_str())?;
    let mut command_line = build_command_line(staged.command.as_os_str(), &spec.args)?;
    let std_handles = inherited_standard_handles()?;
    let _inherit_guard = InheritHandleGuard::new(std_handles.as_slice())?;

    let mut security_capabilities = SECURITY_CAPABILITIES {
        AppContainerSid: profile.sid(),
        Capabilities: null_mut(),
        CapabilityCount: 0,
        Reserved: 0,
    };
    let mut attributes = ProcThreadAttributes::new(2)?;
    attributes.update(
        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
        (&mut security_capabilities as *mut SECURITY_CAPABILITIES).cast(),
        size_of::<SECURITY_CAPABILITIES>(),
    )?;
    attributes.update(
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
        std_handles.as_ptr().cast(),
        std_handles.len() * size_of::<HANDLE>(),
    )?;

    let job = JobHandle::new()?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    startup.StartupInfo.hStdOutput = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    startup.StartupInfo.hStdError = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    startup.lpAttributeList = attributes.as_ptr();
    let mut process = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_ptr().cast(),
            cwd.as_ptr(),
            &startup.StartupInfo,
            &mut process,
        )
    };
    if created == 0 {
        return Err(format!(
            "create Windows AppContainer Plugin process failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let process_guard = ProcessHandleGuard::new(process);
    if unsafe { AssignProcessToJobObject(job.handle, process_guard.process.hProcess) } == 0 {
        unsafe {
            TerminateProcess(process_guard.process.hProcess, 1);
        }
        return Err(format!(
            "assign Windows Plugin process to Job Object failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    if unsafe { ResumeThread(process_guard.process.hThread) } == u32::MAX {
        unsafe {
            TerminateProcess(process_guard.process.hProcess, 1);
        }
        return Err(format!(
            "resume Windows AppContainer Plugin process failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let waited = unsafe { WaitForSingleObject(process_guard.process.hProcess, INFINITE) };
    if waited != WAIT_OBJECT_0 {
        return Err(format!(
            "wait for Windows AppContainer Plugin process failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut exit_code = 1_u32;
    if unsafe { GetExitCodeProcess(process_guard.process.hProcess, &mut exit_code) } == 0 {
        return Err(format!(
            "read Windows AppContainer Plugin exit code failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(i32::try_from(exit_code).unwrap_or(1))
}

fn inherited_standard_handles() -> Result<Vec<HANDLE>, String> {
    let mut handles = Vec::new();
    for standard in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        let handle = unsafe { GetStdHandle(standard) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err("Windows Plugin stdio standard handle is unavailable".to_string());
        }
        if !handles.contains(&handle) {
            handles.push(handle);
        }
    }
    Ok(handles)
}

fn build_command_line(command: &OsStr, args: &[String]) -> Result<Vec<u16>, String> {
    let command = command.encode_wide().collect::<Vec<_>>();
    let args = args
        .iter()
        .map(|argument| argument.encode_utf16().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    windows_command_line::build(command.as_slice(), args.as_slice())
}

fn wide_nul(value: &OsStr) -> Result<Vec<u16>, String> {
    let mut encoded = value.encode_wide().collect::<Vec<_>>();
    if encoded.contains(&0) {
        return Err("Windows Plugin stdio path contains NUL".to_string());
    }
    encoded.push(0);
    Ok(encoded)
}

struct AppContainerProfile {
    name: Vec<u16>,
    sid: PSID,
}

impl AppContainerProfile {
    fn create(sandbox_id: &str) -> Result<Self, String> {
        let profile_name = format!("chatos.plugin.{}", sandbox_id.replace('-', ""));
        let name = wide_nul(OsStr::new(profile_name.as_str()))?;
        let display = wide_nul(OsStr::new("ChatOS Plugin stdio sandbox"))?;
        let description = wide_nul(OsStr::new(
            "Ephemeral offline AppContainer for signed Plugin stdio execution",
        ))?;
        let mut sid = null_mut();
        let result = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                display.as_ptr(),
                description.as_ptr(),
                null(),
                0,
                &mut sid,
            )
        };
        if result < 0 || sid.is_null() {
            return Err(format!(
                "create Windows Plugin AppContainer profile failed with HRESULT 0x{:08x}",
                result as u32
            ));
        }
        Ok(Self { name, sid })
    }

    fn sid(&self) -> PSID {
        self.sid
    }

    fn folder_path(&self) -> Result<PathBuf, String> {
        let mut sid_text = null_mut();
        if unsafe { ConvertSidToStringSidW(self.sid, &mut sid_text) } == 0 {
            return Err(format!(
                "format Windows Plugin AppContainer SID failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut folder = null_mut();
        let result = unsafe { GetAppContainerFolderPath(sid_text, &mut folder) };
        unsafe {
            LocalFree(sid_text.cast());
        }
        if result < 0 || folder.is_null() {
            return Err(format!(
                "locate Windows Plugin AppContainer folder failed with HRESULT 0x{:08x}",
                result as u32
            ));
        }
        let path = PathBuf::from(unsafe { os_string_from_wide_ptr(folder) });
        unsafe {
            CoTaskMemFree(folder.cast());
        }
        Ok(path)
    }
}

impl Drop for AppContainerProfile {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteAppContainerProfile(self.name.as_ptr());
            if !self.sid.is_null() {
                FreeSid(self.sid);
            }
        }
    }
}

unsafe fn os_string_from_wide_ptr(value: *const u16) -> OsString {
    let mut len = 0_usize;
    while *value.add(len) != 0 {
        len += 1;
    }
    OsString::from_wide(std::slice::from_raw_parts(value, len))
}

struct ProcThreadAttributes {
    storage: Vec<usize>,
}

impl ProcThreadAttributes {
    fn new(count: u32) -> Result<Self, String> {
        let mut bytes = 0_usize;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), count, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(format!(
                "size Windows Plugin process attribute list failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let words = bytes.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; words];
        if unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), count, 0, &mut bytes)
        } == 0
        {
            return Err(format!(
                "initialize Windows Plugin process attribute list failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self { storage })
    }

    fn as_ptr(&mut self) -> *mut c_void {
        self.storage.as_mut_ptr().cast()
    }

    fn update(
        &mut self,
        attribute: usize,
        value: *const c_void,
        size: usize,
    ) -> Result<(), String> {
        if unsafe {
            UpdateProcThreadAttribute(self.as_ptr(), 0, attribute, value, size, null_mut(), null())
        } == 0
        {
            return Err(format!(
                "configure Windows Plugin process attribute failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}

impl Drop for ProcThreadAttributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.as_ptr());
        }
    }
}

struct InheritHandleGuard {
    handles: Vec<(HANDLE, u32)>,
}

impl InheritHandleGuard {
    fn new(handles: &[HANDLE]) -> Result<Self, String> {
        let mut configured = Vec::new();
        for handle in handles {
            let mut original = 0_u32;
            if unsafe { GetHandleInformation(*handle, &mut original) } == 0 {
                restore_handle_flags(configured.as_slice());
                return Err(format!(
                    "read Windows Plugin stdio handle flags failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if unsafe { SetHandleInformation(*handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) }
                == 0
            {
                restore_handle_flags(configured.as_slice());
                return Err(format!(
                    "make Windows Plugin stdio handle inheritable failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            configured.push((*handle, original));
        }
        Ok(Self {
            handles: configured,
        })
    }
}

impl Drop for InheritHandleGuard {
    fn drop(&mut self) {
        restore_handle_flags(self.handles.as_slice());
    }
}

fn restore_handle_flags(handles: &[(HANDLE, u32)]) {
    for (handle, flags) in handles.iter().rev() {
        unsafe {
            let _ =
                SetHandleInformation(*handle, HANDLE_FLAG_INHERIT, *flags & HANDLE_FLAG_INHERIT);
        }
    }
}

struct JobHandle {
    handle: HANDLE,
}

impl JobHandle {
    fn new() -> Result<Self, String> {
        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return Err(format!(
                "create Windows Plugin Job Object failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        limits.BasicLimitInformation.ActiveProcessLimit = JOB_ACTIVE_PROCESS_LIMIT;
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            let error = std::io::Error::last_os_error();
            unsafe {
                CloseHandle(handle);
            }
            return Err(format!(
                "configure Windows Plugin Job Object failed: {error}"
            ));
        }
        let restrictions = JOBOBJECT_BASIC_UI_RESTRICTIONS {
            UIRestrictionsClass: JOB_OBJECT_UILIMIT_HANDLES
                | JOB_OBJECT_UILIMIT_READCLIPBOARD
                | JOB_OBJECT_UILIMIT_WRITECLIPBOARD
                | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS
                | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS
                | JOB_OBJECT_UILIMIT_GLOBALATOMS
                | JOB_OBJECT_UILIMIT_DESKTOP
                | JOB_OBJECT_UILIMIT_EXITWINDOWS,
        };
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectBasicUIRestrictions,
                (&restrictions as *const JOBOBJECT_BASIC_UI_RESTRICTIONS).cast(),
                size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32,
            )
        } == 0
        {
            let error = std::io::Error::last_os_error();
            unsafe {
                CloseHandle(handle);
            }
            return Err(format!(
                "configure Windows Plugin Job UI restrictions failed: {error}"
            ));
        }
        Ok(Self { handle })
    }
}

impl Drop for JobHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

struct ProcessHandleGuard {
    process: PROCESS_INFORMATION,
}

impl ProcessHandleGuard {
    fn new(process: PROCESS_INFORMATION) -> Self {
        Self { process }
    }
}

impl Drop for ProcessHandleGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.process.hThread.is_null() {
                CloseHandle(self.process.hThread);
            }
            if !self.process.hProcess.is_null() {
                CloseHandle(self.process.hProcess);
            }
        }
    }
}
