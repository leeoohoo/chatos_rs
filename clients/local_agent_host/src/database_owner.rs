// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use fs2::FileExt;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Seek, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// A cooperative Host ownership lock, acquired before opening SQLite and held
/// until the process exits. Never delete the persistent lock file: unlinking it
/// would allow a new process to lock a different inode while this one is alive.
#[derive(Debug)]
pub(super) struct DatabaseOwner {
    _file: File,
    database: PathBuf,
}

#[derive(Serialize)]
struct OwnerDescription<'a> {
    format_version: u32,
    owner_epoch: String,
    pid: u32,
    database: &'a Path,
}

impl DatabaseOwner {
    pub(super) fn acquire(database: &Path) -> io::Result<Self> {
        let configured_database = database.to_path_buf();
        let database = resolve_database(database)?;
        let lock_path = lock_path(&database);
        let mut options = OpenOptions::new();
        // No truncation until after successful ownership acquisition.
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let mut file = options.open(&lock_path)?;
        validate_lock_file(&file, &lock_path)?;
        FileExt::try_lock_exclusive(&file).map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "Local Agent database is already owned by another Host; stop the previous \
                     Host before opening or migrating it (do not remove the owner lock file)",
                )
            } else {
                error
            }
        })?;
        // Reject replacement during acquisition. The parent directory must be
        // trusted; this advisory protocol cannot fence a process that unlinks
        // the lock after acquisition or bypasses the protocol entirely.
        validate_lock_file(&file, &lock_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        let description = OwnerDescription {
            format_version: 1,
            owner_epoch: Uuid::new_v4().to_string(),
            pid: std::process::id(),
            database: &database,
        };
        let json = serde_json::to_vec(&description)?;
        file.rewind()?;
        file.set_len(0)?;
        file.write_all(&json)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(Self {
            _file: file,
            // Preserve the configured path for SQLite's existing artifact-root
            // convention. Canonicalization is for lock identity/diagnostics;
            // changing a filename symlink to its target here would silently
            // move the associated `.artifacts` directory.
            database: configured_database,
        })
    }

    pub(super) fn database(&self) -> &Path {
        &self.database
    }
}

fn resolve_database(database: &Path) -> io::Result<PathBuf> {
    // Resolve both an existing DB symlink and parent-directory aliases for lock
    // identity. The configured path and its parents must remain trusted/stable.
    match fs::metadata(database) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "database must be a regular file",
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() != 1 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "hard-linked databases are not supported for Host ownership",
                    ));
                }
            }
            database.canonicalize()
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A dangling symlink is not a new database. Do not follow it later
            // through a path whose lock was derived from a different target.
            if fs::symlink_metadata(database).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "database symlink has no target",
                ));
            }
            let parent = database
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            fs::create_dir_all(parent)?;
            let name = database.file_name().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "database path needs a file name",
                )
            })?;
            Ok(parent.canonicalize()?.join(name))
        }
        Err(error) => Err(error),
    }
}

fn lock_path(database: &Path) -> PathBuf {
    let mut name = database.as_os_str().to_os_string();
    name.push(".owner.lock");
    PathBuf::from(name)
}

fn validate_lock_file(file: &File, path: &Path) -> io::Result<()> {
    let opened = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if !opened.is_file() || !named.is_file() || named.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "database owner lock must be a regular, non-symlink file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.nlink() != 1 || opened.dev() != named.dev() || opened.ino() != named.ino() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "database owner lock was replaced or hard-linked",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
