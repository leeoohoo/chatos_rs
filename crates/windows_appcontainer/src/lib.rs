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

    /// Creates an application-owned directory tree without ever following a
    /// symbolic link or Windows reparse point in an existing component.
    pub fn ensure_owned_directory(path: &Path) -> io::Result<()> {
        match fs::symlink_metadata(path) {
            Ok(_) => validate_directory(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let parent = path.parent().ok_or_else(invalid_input)?;
                ensure_owned_directory(parent)?;
                match fs::create_dir(path) {
                    Ok(()) => validate_directory(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        validate_directory(path)
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    pub fn create_owned_file(
        directory: &Path,
        destination: &Path,
        temporary_prefix: &str,
        bytes: &[u8],
    ) -> io::Result<()> {
        ensure_owned_directory(directory)?;
        if destination.parent() != Some(directory) || fs::symlink_metadata(destination).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination is not new",
            ));
        }
        let temporary = directory.join(format!("{temporary_prefix}{}.tmp", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let mut file = options.open(&temporary)?;
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

    fn validate_directory(path: &Path) -> io::Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && !is_windows_reparse_point(&metadata)
        {
            Ok(())
        } else {
            Err(invalid_input())
        }
    }

    fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    fn invalid_input() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, "invalid app data transaction")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn owned_directory_creation_rejects_file_components() {
            let root = tempfile::tempdir().unwrap();
            let nested = root.path().join("one").join("two");
            ensure_owned_directory(&nested).unwrap();
            assert!(nested.is_dir());

            let file = root.path().join("file");
            fs::write(&file, b"not a directory").unwrap();
            assert!(ensure_owned_directory(&file.join("child")).is_err());
        }

        #[test]
        fn owned_file_creation_is_new_only_and_cleans_temporary_on_collision() {
            let root = tempfile::tempdir().unwrap();
            let destination = root.path().join("journal.json");
            create_owned_file(root.path(), &destination, ".journal-", b"first").unwrap();
            assert_eq!(fs::read(&destination).unwrap(), b"first");
            assert!(create_owned_file(root.path(), &destination, ".journal-", b"second").is_err());
            assert_eq!(fs::read(&destination).unwrap(), b"first");
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
        }
    }
}

#[cfg(windows)]
mod terminal_runtime {
    pub mod process {
        use std::{
            io,
            os::windows::io::{AsRawHandle, FromRawHandle, RawHandle},
        };
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
                let owned =
                    unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(job as RawHandle) };
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
