// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

pub(crate) fn open_directory_without_symlinks(path: &Path) -> std::io::Result<fs::File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Component;

    let mut components = path.components();
    if components.next() != Some(Component::RootDir) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "expected an absolute canonical directory path",
        ));
    }
    let flags = libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC;
    let mut directory = fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open("/")?;
    for component in components {
        let Component::Normal(name) = component else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "expected only normal directory components",
            ));
        };
        let name = CString::new(name.as_bytes())?;
        // Walk one component relative to a held directory descriptor. A renamed
        // or replaced ancestor cannot redirect subsequent opens.
        // SAFETY: directory owns a live descriptor; name is a NUL-terminated
        // single component. O_CREAT is absent, so no mode argument is required.
        let fd =
            unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags | libc::O_RDONLY) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: successful openat returns a new descriptor owned only here.
        directory = unsafe { fs::File::from_raw_fd(fd) };
    }
    Ok(directory)
}
