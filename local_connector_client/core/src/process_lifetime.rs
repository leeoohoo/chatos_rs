// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

pub(crate) struct ProcessLifetimeGuard {
    #[cfg(windows)]
    _job: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(not(windows))]
pub(crate) fn attach_current_process_tree() -> Result<ProcessLifetimeGuard> {
    attach_parent_watchdog()?;
    Ok(ProcessLifetimeGuard {})
}

#[cfg(not(windows))]
fn attach_parent_watchdog() -> Result<()> {
    use std::thread;
    use std::time::Duration;

    let Some(expected_parent) = std::env::var("LOCAL_CONNECTOR_PARENT_PID")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(());
    };
    let expected_parent = expected_parent
        .parse::<libc::pid_t>()
        .context("parse LOCAL_CONNECTOR_PARENT_PID")?;
    if expected_parent <= 1 {
        anyhow::bail!("LOCAL_CONNECTOR_PARENT_PID must identify a live parent process");
    }

    thread::Builder::new()
        .name("local-connector-parent-watchdog".to_string())
        .spawn(move || loop {
            thread::sleep(Duration::from_secs(2));
            let current_parent = unsafe { libc::getppid() };
            if should_terminate_for_parent(expected_parent, current_parent) {
                std::process::exit(0);
            }
        })
        .context("start Local Connector parent-process watchdog")?;
    Ok(())
}

#[cfg(not(windows))]
fn should_terminate_for_parent(expected_parent: libc::pid_t, current_parent: libc::pid_t) -> bool {
    current_parent <= 1 || current_parent != expected_parent
}

#[cfg(windows)]
pub(crate) fn attach_current_process_tree() -> Result<ProcessLifetimeGuard> {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::null;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let job = unsafe { CreateJobObjectW(null(), null()) };
    if job.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const c_void,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if configured == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            CloseHandle(job);
        }
        return Err(error.into());
    }
    let assigned = unsafe { AssignProcessToJobObject(job, GetCurrentProcess()) };
    if assigned == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            CloseHandle(job);
        }
        return Err(error.into());
    }
    // The handle intentionally remains open for the lifetime of the Core process. Windows closes
    // it during process teardown, which atomically terminates every descendant still in the job.
    Ok(ProcessLifetimeGuard { _job: job })
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::should_terminate_for_parent;

    #[test]
    fn unix_core_stops_after_its_desktop_parent_disappears() {
        assert!(!should_terminate_for_parent(42, 42));
        assert!(should_terminate_for_parent(42, 1));
        assert!(should_terminate_for_parent(42, 43));
    }
}
