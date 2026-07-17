// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::c_void;
use std::fs;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::ptr::{null_mut, NonNull};

use anyhow::{anyhow, Context, Result};
use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS, GENERIC_ALL, GENERIC_WRITE};
use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    AclSizeInformation, GetAce, GetAclInformation, IsWellKnownSid, WinBuiltinAdministratorsSid,
    WinLocalSystemSid, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION,
    DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_APPEND_DATA, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
    ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE, ACCESS_ALLOWED_COMPOUND_ACE_TYPE,
    ACCESS_ALLOWED_OBJECT_ACE_TYPE,
};

const UNTRUSTED_WRITE_ACCESS: u32 = FILE_WRITE_DATA
    | FILE_APPEND_DATA
    | FILE_WRITE_EA
    | FILE_DELETE_CHILD
    | FILE_WRITE_ATTRIBUTES
    | DELETE
    | WRITE_DAC
    | WRITE_OWNER
    | GENERIC_WRITE
    | GENERIC_ALL;

pub(crate) fn validate_windows_secure_system_path(path: &Path, label: &str) -> Result<()> {
    let absolute = absolute_path(path)?;
    reject_reparse_path_components(absolute.as_path(), label)?;
    validate_owner_and_dacl(absolute.as_path(), label)?;
    let parent = absolute
        .parent()
        .ok_or_else(|| anyhow!("{label} {} has no parent directory", absolute.display()))?;
    validate_owner_and_dacl(parent, format!("{label} directory").as_str())?;
    Ok(())
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("resolve current directory for secure system file")?
            .join(path))
    }
}

fn reject_reparse_path_components(path: &Path, label: &str) -> Result<()> {
    for component in path.ancestors() {
        let metadata = fs::symlink_metadata(component)
            .with_context(|| format!("read {label} path component {}", component.display()))?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(anyhow!(
                "{label} path component {} must not be a Windows reparse point",
                component.display()
            ));
        }
    }
    Ok(())
}

fn validate_owner_and_dacl(path: &Path, label: &str) -> Result<()> {
    let wide_path = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut owner: PSID = null_mut();
    let mut dacl: *mut ACL = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(std::io::Error::from_raw_os_error(status as i32))
            .with_context(|| format!("read {label} ACL {}", path.display()));
    }
    let descriptor = LocalSecurityDescriptor::new(descriptor)
        .ok_or_else(|| anyhow!("{label} {} has no security descriptor", path.display()))?;
    let _descriptor = descriptor;

    if owner.is_null() || !sid_is_system_or_administrators(owner) {
        return Err(anyhow!(
            "{label} {} must be owned by LocalSystem or BUILTIN\\Administrators",
            path.display()
        ));
    }
    if dacl.is_null() {
        return Err(anyhow!(
            "{label} {} must have a restrictive DACL",
            path.display()
        ));
    }
    validate_dacl(path, label, dacl)
}

fn validate_dacl(path: &Path, label: &str, dacl: *const ACL) -> Result<()> {
    let mut information = ACL_SIZE_INFORMATION::default();
    let ok = unsafe {
        GetAclInformation(
            dacl,
            &mut information as *mut ACL_SIZE_INFORMATION as *mut c_void,
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("inspect {label} DACL {}", path.display()));
    }

    for index in 0..information.AceCount {
        let mut ace_pointer = null_mut();
        if unsafe { GetAce(dacl, index, &mut ace_pointer) } == 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("read {label} DACL entry {}", path.display()));
        }
        let header = unsafe { &*(ace_pointer as *const ACE_HEADER) };
        if !is_allow_ace(header.AceType) {
            continue;
        }
        if (header.AceSize as usize) < size_of::<ACCESS_ALLOWED_ACE>() {
            return Err(anyhow!(
                "{label} {} contains a malformed allow ACE",
                path.display()
            ));
        }
        let allowed = unsafe { &*(ace_pointer as *const ACCESS_ALLOWED_ACE) };
        if allowed.Mask & UNTRUSTED_WRITE_ACCESS == 0 {
            continue;
        }
        if matches!(
            header.AceType as u32,
            ACCESS_ALLOWED_ACE_TYPE | ACCESS_ALLOWED_CALLBACK_ACE_TYPE
        ) {
            let sid = &allowed.SidStart as *const u32 as PSID;
            if sid_is_system_or_administrators(sid) {
                continue;
            }
        }
        return Err(anyhow!(
            "{label} {} grants write or ownership control to a non-administrative principal",
            path.display()
        ));
    }
    Ok(())
}

fn is_allow_ace(ace_type: u8) -> bool {
    matches!(
        ace_type as u32,
        ACCESS_ALLOWED_ACE_TYPE
            | ACCESS_ALLOWED_CALLBACK_ACE_TYPE
            | ACCESS_ALLOWED_OBJECT_ACE_TYPE
            | ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE
            | ACCESS_ALLOWED_COMPOUND_ACE_TYPE
    )
}

fn sid_is_system_or_administrators(sid: PSID) -> bool {
    !sid.is_null()
        && unsafe {
            IsWellKnownSid(sid, WinLocalSystemSid) != 0
                || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
        }
}

struct LocalSecurityDescriptor(NonNull<c_void>);

impl LocalSecurityDescriptor {
    fn new(descriptor: PSECURITY_DESCRIPTOR) -> Option<Self> {
        NonNull::new(descriptor).map(Self)
    }
}

impl Drop for LocalSecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0.as_ptr());
        }
    }
}
