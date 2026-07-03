// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::path::Path;

use sha2::{Digest, Sha256};

const DEFAULT_UID_BASE: u32 = 200_000;
const DEFAULT_UID_SPAN: u32 = 1_000_000_000;
const DEFAULT_GID_BASE: u32 = 200_000;
const DEFAULT_GID_SPAN: u32 = 1_000_000_000;
const DEFAULT_CHOWN_MAX_ENTRIES: usize = 200_000;
#[cfg(target_os = "linux")]
const CAP_CHOWN: u32 = 0;
#[cfg(target_os = "linux")]
const CAP_SETGID: u32 = 6;
#[cfg(target_os = "linux")]
const CAP_SETUID: u32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProcessIsolationSpec {
    uid: u32,
    gid: u32,
}

#[derive(Debug, Clone)]
struct ProcessIsolationConfig {
    enabled: bool,
    uid_base: u32,
    uid_span: u32,
    gid_base: u32,
    gid_span: u32,
    chown_workspace: bool,
    chown_max_entries: usize,
}

impl ProcessIsolationConfig {
    fn from_env() -> Result<Self, String> {
        let enabled = env_bool("CHATOS_PROCESS_ISOLATION_ENABLED");
        let uid_base = env_u32("CHATOS_PROCESS_ISOLATION_UID_BASE", DEFAULT_UID_BASE)?;
        let uid_span = env_u32("CHATOS_PROCESS_ISOLATION_UID_SPAN", DEFAULT_UID_SPAN)?;
        let gid_base = env_u32("CHATOS_PROCESS_ISOLATION_GID_BASE", DEFAULT_GID_BASE)?;
        let gid_span = env_u32("CHATOS_PROCESS_ISOLATION_GID_SPAN", DEFAULT_GID_SPAN)?;
        let chown_workspace = env_bool_default("CHATOS_PROCESS_ISOLATION_CHOWN_WORKSPACE", true);
        let chown_max_entries = env_usize(
            "CHATOS_PROCESS_ISOLATION_CHOWN_MAX_ENTRIES",
            DEFAULT_CHOWN_MAX_ENTRIES,
        )?;
        validate_id_range("uid", uid_base, uid_span)?;
        validate_id_range("gid", gid_base, gid_span)?;
        Ok(Self {
            enabled,
            uid_base,
            uid_span,
            gid_base,
            gid_span,
            chown_workspace,
            chown_max_entries,
        })
    }
}

pub(crate) fn resolve_for_user(
    user_id: Option<&str>,
) -> Result<Option<ProcessIsolationSpec>, String> {
    let cfg = ProcessIsolationConfig::from_env()?;
    if !cfg.enabled {
        return Ok(None);
    }

    if !cfg!(target_os = "linux") {
        return Err("OS 用户级进程隔离仅支持 Linux".to_string());
    }

    let user_id = user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OS 用户级进程隔离已开启，但缺少 user_id".to_string())?;
    let hash = stable_user_hash(user_id);
    let spec = ProcessIsolationSpec {
        uid: map_id(hash, cfg.uid_base, cfg.uid_span),
        gid: map_id(hash, cfg.gid_base, cfg.gid_span),
    };
    ensure_can_apply(spec)?;
    Ok(Some(spec))
}

pub(crate) fn prepare_workspace_for_user(
    path: &Path,
    spec: Option<&ProcessIsolationSpec>,
) -> Result<(), String> {
    let Some(spec) = spec else {
        return Ok(());
    };
    let cfg = ProcessIsolationConfig::from_env()?;
    if !cfg.enabled || !cfg.chown_workspace {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        chown_tree(path, spec.uid, spec.gid, cfg.chown_max_entries)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = spec;
        let _ = cfg.chown_max_entries;
        let _ = path;
        Err("OS 用户级进程隔离仅支持 Linux".to_string())
    }
}

pub(crate) fn apply_to_tokio_command(
    cmd: &mut tokio::process::Command,
    spec: Option<&ProcessIsolationSpec>,
) -> Result<(), String> {
    let Some(spec) = spec else {
        return Ok(());
    };
    ensure_can_apply(*spec)?;

    #[cfg(target_os = "linux")]
    {
        let uid = spec.uid;
        let gid = spec.gid;
        let clear_groups = should_clear_groups_linux();
        unsafe {
            cmd.pre_exec(move || apply_current_process_linux(uid, gid, clear_groups));
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = cmd;
        Err("OS 用户级进程隔离仅支持 Linux".to_string())
    }
}

fn stable_user_hash(user_id: &str) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(user_id.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes)
}

fn map_id(hash: u64, base: u32, span: u32) -> u32 {
    base.saturating_add((hash % u64::from(span)) as u32)
}

fn validate_id_range(label: &str, base: u32, span: u32) -> Result<(), String> {
    if span == 0 {
        return Err(format!(
            "CHATOS_PROCESS_ISOLATION_{}_SPAN 必须大于 0",
            label.to_ascii_uppercase()
        ));
    }
    if base == 0 {
        return Err(format!(
            "CHATOS_PROCESS_ISOLATION_{}_BASE 不能为 0",
            label.to_ascii_uppercase()
        ));
    }
    let max = u64::from(base) + u64::from(span) - 1;
    if max > u64::from(u32::MAX) {
        return Err(format!(
            "CHATOS_PROCESS_ISOLATION_{}_BASE/SPAN 超出 u32 范围",
            label.to_ascii_uppercase()
        ));
    }
    Ok(())
}

fn env_bool(key: &str) -> bool {
    env_bool_default(key, false)
}

fn env_bool_default(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn env_u32(key: &str, default: u32) -> Result<u32, String> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u32>()
            .map_err(|err| format!("{key} 必须是 u32: {err}")),
        _ => Ok(default),
    }
}

fn env_usize(key: &str, default: usize) -> Result<usize, String> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<usize>()
            .map_err(|err| format!("{key} 必须是 usize: {err}")),
        _ => Ok(default),
    }
}

#[cfg(target_os = "linux")]
fn ensure_can_apply(spec: ProcessIsolationSpec) -> Result<(), String> {
    let euid = unsafe { libc::geteuid() };
    let egid = unsafe { libc::getegid() };
    let can_set_identity =
        euid == 0 || (has_effective_cap(CAP_SETUID) && has_effective_cap(CAP_SETGID));
    if !can_set_identity && (euid != spec.uid || egid != spec.gid) {
        return Err(format!(
            "OS 用户级进程隔离需要服务进程以 root 运行，或具备 CAP_SETUID/CAP_SETGID；当前 uid/gid={euid}/{egid}, 目标 uid/gid={}/{}",
            spec.uid, spec.gid
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn ensure_can_apply(_spec: ProcessIsolationSpec) -> Result<(), String> {
    Err("OS 用户级进程隔离仅支持 Linux".to_string())
}

#[cfg(target_os = "linux")]
fn chown_tree(path: &Path, uid: u32, gid: u32, max_entries: usize) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;

    let mut count = 0usize;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|err| err.to_string())?;
        count = count.saturating_add(1);
        if count > max_entries {
            return Err(format!(
                "OS 用户级进程隔离 chown 超过上限: entries>{max_entries}"
            ));
        }
        let metadata = std::fs::symlink_metadata(entry.path()).map_err(|err| err.to_string())?;
        if metadata.uid() == uid && metadata.gid() == gid {
            continue;
        }
        if unsafe { libc::geteuid() } != 0 && !has_effective_cap(CAP_CHOWN) {
            return Err("OS 用户级进程隔离需要 root 或 CAP_CHOWN 才能准备工作目录属主".to_string());
        }
        lchown_path(entry.path(), uid, gid)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn lchown_path(path: &Path, uid: u32, gid: u32) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("path contains NUL byte: {}", path.display()))?;
    let rc = unsafe { libc::lchown(c_path.as_ptr(), uid, gid) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "chown {} failed: {}",
            path.display(),
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(target_os = "linux")]
fn apply_current_process_linux(uid: u32, gid: u32, clear_groups: bool) -> std::io::Result<()> {
    unsafe {
        libc::umask(0o077);
        let _ = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
        if clear_groups && libc::setgroups(0, std::ptr::null()) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::getegid() != gid && libc::setgid(gid) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::geteuid() != uid && libc::setuid(uid) != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn should_clear_groups_linux() -> bool {
    (unsafe { libc::geteuid() }) == 0 || has_effective_cap(CAP_SETGID)
}

#[cfg(target_os = "linux")]
fn has_effective_cap(cap: u32) -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    for line in status.lines() {
        let Some(raw) = line.strip_prefix("CapEff:") else {
            continue;
        };
        let Ok(bits) = u64::from_str_radix(raw.trim(), 16) else {
            return false;
        };
        return bits & (1u64 << cap) != 0;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{map_id, stable_user_hash, DEFAULT_UID_BASE, DEFAULT_UID_SPAN};

    #[test]
    fn process_isolation_user_hash_is_stable() {
        assert_eq!(stable_user_hash("user-a"), stable_user_hash("user-a"));
        assert_ne!(stable_user_hash("user-a"), stable_user_hash("user-b"));
    }

    #[test]
    fn process_isolation_uid_stays_in_configured_range() {
        let uid = map_id(
            stable_user_hash("user-a"),
            DEFAULT_UID_BASE,
            DEFAULT_UID_SPAN,
        );
        assert!(uid >= DEFAULT_UID_BASE);
        assert!(uid < DEFAULT_UID_BASE + DEFAULT_UID_SPAN);
    }
}
