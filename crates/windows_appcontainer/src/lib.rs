// SPDX-License-Identifier: PolyForm-Noncommercial-1.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Shared Windows AppContainer runtime used by the Tauri client and Local Agent Host.

#![allow(dead_code)]

#[cfg(windows)]
mod app_data_maintenance {
    use std::{fs, path::Path, time::SystemTime};

    pub fn try_remove_stale_owned_directory_tree(
        root: &Path,
        cutoff: SystemTime,
        maximum_entries: usize,
    ) -> bool {
        if maximum_entries == 0 || !is_stale_safe_tree(root, cutoff, maximum_entries) {
            return false;
        }
        fs::remove_dir_all(root).is_ok()
    }

    fn is_stale_safe_tree(root: &Path, cutoff: SystemTime, maximum_entries: usize) -> bool {
        let mut pending = vec![root.to_path_buf()];
        let mut visited = 0usize;
        while let Some(path) = pending.pop() {
            visited = visited.saturating_add(1);
            if visited > maximum_entries {
                return false;
            }
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                return false;
            };
            if metadata.file_type().is_symlink()
                || metadata.modified().is_err_and(|_| true)
                || metadata.modified().is_ok_and(|modified| modified > cutoff)
            {
                return false;
            }
            if metadata.is_file() {
                continue;
            }
            if !metadata.is_dir() {
                return false;
            }
            let Ok(entries) = fs::read_dir(path) else {
                return false;
            };
            for entry in entries {
                let Ok(entry) = entry else {
                    return false;
                };
                pending.push(entry.path());
            }
        }
        true
    }
}

#[cfg(windows)]
mod app_data_transaction {
    use std::{
        fs::{self, OpenOptions},
        io::{self, Write},
        path::Path,
    };

    pub fn create_owned_file(
        directory: &Path,
        destination: &Path,
        temporary_prefix: &str,
        bytes: &[u8],
    ) -> io::Result<()> {
        fs::create_dir_all(directory)?;
        if destination.parent() != Some(directory) || destination.exists() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "destination is not new"));
        }
        let temporary = directory.join(format!("{temporary_prefix}{}.tmp", uuid::Uuid::new_v4()));
        let mut file = OpenOptions::new().create_new(true).write(true).open(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        if let Err(error) = fs::rename(&temporary, destination) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(windows)]
mod terminal_runtime {
    pub mod process {
        use std::{io, os::windows::io::{AsRawHandle, FromRawHandle, RawHandle}};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        pub struct ProcessTermination(std::os::windows::io::OwnedHandle);

        impl ProcessTermination {
            pub fn attach_raw_handle(raw: RawHandle) -> io::Result<Self> {
                let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
                if job.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let owned = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(job as RawHandle) };
                let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let configured = unsafe {
                    SetInformationJobObject(
                        job,
                        JobObjectExtendedLimitInformation,
                        &limits as *const _ as *const std::ffi::c_void,
                        std::mem::size_of_val(&limits) as u32,
                    )
                };
                if configured == 0 {
                    return Err(io::Error::last_os_error());
                }
                if unsafe { AssignProcessToJobObject(job, raw as _) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Self(owned))
            }

            pub fn terminate(&self, _force: bool) -> io::Result<()> {
                let result = unsafe { TerminateJobObject(self.0.as_raw_handle() as _, 1) };
                if result == 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            }
        }

        impl Drop for ProcessTermination {
            fn drop(&mut self) {
                let _ = self.terminate(true);
            }
        }
    }
}

#[cfg(windows)]
#[path = "../../../clients/windows-react/src-tauri/src/windows_appcontainer.rs"]
mod implementation;

#[cfg(windows)]
pub use implementation::{
    restricted_runtime_environment, SandboxedChild, WorkspaceAccess, WorkspaceLeaseManager,
};
