// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;

#[derive(Debug)]
pub(in crate::command_sandbox) struct TransientPath {
    path: PathBuf,
    kind: TransientPathKind,
    identity: Option<FileIdentity>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub(in crate::command_sandbox) enum TransientPathKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::command_sandbox) struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl TransientPath {
    #[cfg(target_os = "linux")]
    pub(in crate::command_sandbox) fn create_directory(path: &Path) -> Result<Self, String> {
        std::fs::create_dir(path).map_err(|err| {
            format!(
                "create protected mount target {} failed: {err}",
                path.display()
            )
        })?;
        let identity = file_identity(path);
        Ok(Self {
            path: path.to_path_buf(),
            kind: TransientPathKind::Directory,
            identity,
        })
    }

    #[cfg(target_os = "linux")]
    pub(in crate::command_sandbox) fn create_file(path: &Path) -> Result<Self, String> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|err| {
                format!(
                    "create denied mount target {} failed: {err}",
                    path.display()
                )
            })?;
        let identity = file_identity(path);
        Ok(Self {
            path: path.to_path_buf(),
            kind: TransientPathKind::File,
            identity,
        })
    }

    pub(in crate::command_sandbox) fn remove_if_unchanged(self) {
        if self.identity.is_some() && file_identity(self.path.as_path()) != self.identity {
            return;
        }
        match self.kind {
            TransientPathKind::File => {
                let _ = std::fs::remove_file(self.path);
            }
            TransientPathKind::Directory => {
                let _ = std::fs::remove_dir(self.path);
            }
        }
    }
}

#[cfg(unix)]
pub(in crate::command_sandbox) fn file_identity(path: &Path) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::symlink_metadata(path).ok()?;
    Some(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(not(unix))]
pub(in crate::command_sandbox) fn file_identity(_path: &Path) -> Option<FileIdentity> {
    None
}
