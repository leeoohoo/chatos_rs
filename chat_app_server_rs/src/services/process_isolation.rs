// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use sha2::{Digest, Sha256};

const EXEC_HELPER_ARG: &str = "--chatos-process-isolation-exec";
const HELPER_ENV_KEYS: &[&str] = &[
    "CHATOS_PROCESS_ISOLATION_ENABLED",
    "CHATOS_PROCESS_ISOLATION_UID_BASE",
    "CHATOS_PROCESS_ISOLATION_UID_SPAN",
    "CHATOS_PROCESS_ISOLATION_GID_BASE",
    "CHATOS_PROCESS_ISOLATION_GID_SPAN",
    "CHATOS_PROCESS_ISOLATION_CHOWN_WORKSPACE",
    "CHATOS_PROCESS_ISOLATION_CHOWN_MAX_ENTRIES",
    "CHATOS_PROCESS_ISOLATION_FS_ENABLED",
    "CHATOS_PROCESS_ISOLATION_FS_ROOT",
    "CHATOS_PROCESS_ISOLATION_FS_MOUNT_PROC",
];
const DEFAULT_UID_BASE: u32 = 200_000;
const DEFAULT_UID_SPAN: u32 = 1_000_000_000;
const DEFAULT_GID_BASE: u32 = 200_000;
const DEFAULT_GID_SPAN: u32 = 1_000_000_000;
const DEFAULT_CHOWN_MAX_ENTRIES: usize = 200_000;
const GUEST_WORKSPACE: &str = "/workspace";
const GUEST_HOME: &str = "/home/chatos";
const GUEST_TMP: &str = "/tmp";
#[cfg(target_os = "linux")]
const DEFAULT_FS_ROOT: &str = "/tmp/chatos-process-isolation";
#[cfg(target_os = "linux")]
const PIVOT_OLD_ROOT: &str = ".pivot-old-root";
#[cfg(target_os = "linux")]
const CAP_CHOWN: u32 = 0;
#[cfg(target_os = "linux")]
const CAP_DAC_READ_SEARCH: u32 = 2;
#[cfg(target_os = "linux")]
const CAP_FOWNER: u32 = 3;
#[cfg(target_os = "linux")]
const CAP_SETGID: u32 = 6;
#[cfg(target_os = "linux")]
const CAP_SETUID: u32 = 7;
#[cfg(target_os = "linux")]
const CAP_SYS_ADMIN: u32 = 21;
#[cfg(target_os = "linux")]
const LINUX_CAPABILITY_VERSION_3: u32 = 0x2008_0522;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProcessIsolationSpec {
    uid: u32,
    gid: u32,
}

impl ProcessIsolationSpec {
    pub(crate) fn login_name(self) -> String {
        format!("chatos-u{}", self.uid)
    }
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
    fs_enabled: bool,
    #[cfg(target_os = "linux")]
    fs_root: PathBuf,
    #[cfg(target_os = "linux")]
    fs_mount_proc: bool,
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
        let fs_enabled = env_bool_default("CHATOS_PROCESS_ISOLATION_FS_ENABLED", false);
        #[cfg(target_os = "linux")]
        let fs_root = env_path("CHATOS_PROCESS_ISOLATION_FS_ROOT", DEFAULT_FS_ROOT);
        #[cfg(target_os = "linux")]
        let fs_mount_proc = env_bool_default("CHATOS_PROCESS_ISOLATION_FS_MOUNT_PROC", false);

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
            fs_enabled,
            #[cfg(target_os = "linux")]
            fs_root,
            #[cfg(target_os = "linux")]
            fs_mount_proc,
        })
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct FilesystemViewSpec {
    uid: u32,
    gid: u32,
    root: PathBuf,
    cwd: Option<PathBuf>,
    home: Option<PathBuf>,
    terminal_shell: Option<PathBuf>,
    mount_proc: bool,
}

pub(crate) fn resolve_for_user(
    user_id: Option<&str>,
) -> Result<Option<ProcessIsolationSpec>, String> {
    let cfg = ProcessIsolationConfig::from_env()?;
    if !cfg.enabled {
        return Ok(None);
    }

    if !cfg!(target_os = "linux") {
        return Err("OS 鐢ㄦ埛绾ц繘绋嬮殧绂讳粎鏀寔 Linux".to_string());
    }

    let user_id = user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OS 鐢ㄦ埛绾ц繘绋嬮殧绂诲凡寮€鍚紝浣嗙己灏?user_id".to_string())?;
    let hash = stable_user_hash(user_id);
    let spec = ProcessIsolationSpec {
        uid: map_id(hash, cfg.uid_base, cfg.uid_span),
        gid: map_id(hash, cfg.gid_base, cfg.gid_span),
    };
    ensure_can_apply(spec, cfg.fs_enabled)?;
    Ok(Some(spec))
}

pub(crate) fn filesystem_view_enabled(spec: Option<&ProcessIsolationSpec>) -> Result<bool, String> {
    if spec.is_none() {
        return Ok(false);
    }
    Ok(ProcessIsolationConfig::from_env()?.fs_enabled)
}

pub(crate) fn child_cwd_for(
    spec: Option<&ProcessIsolationSpec>,
    host_cwd: &Path,
) -> Result<String, String> {
    if filesystem_view_enabled(spec)? {
        Ok(GUEST_WORKSPACE.to_string())
    } else {
        Ok(host_cwd.to_string_lossy().into_owned())
    }
}

pub(crate) fn child_home_for(
    spec: Option<&ProcessIsolationSpec>,
    host_home: &Path,
) -> Result<String, String> {
    if filesystem_view_enabled(spec)? {
        Ok(GUEST_HOME.to_string())
    } else {
        Ok(host_home.to_string_lossy().into_owned())
    }
}

pub(crate) fn child_tmp_for(
    spec: Option<&ProcessIsolationSpec>,
    host_tmp: &Path,
) -> Result<String, String> {
    if filesystem_view_enabled(spec)? {
        Ok(GUEST_TMP.to_string())
    } else {
        Ok(host_tmp.to_string_lossy().into_owned())
    }
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
        prepare_user_scope_for_user(path, spec.uid, spec.gid)?;
        if path_owner_matches(path, spec.uid, spec.gid)? {
            return Ok(());
        }
        chown_tree(path, spec.uid, spec.gid, cfg.chown_max_entries)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = spec;
        let _ = cfg.chown_max_entries;
        let _ = path;
        Err("OS 鐢ㄦ埛绾ц繘绋嬮殧绂讳粎鏀寔 Linux".to_string())
    }
}

pub(crate) fn apply_to_tokio_command(
    cmd: &mut tokio::process::Command,
    spec: Option<&ProcessIsolationSpec>,
    cwd: Option<&Path>,
    home: Option<&Path>,
) -> Result<(), String> {
    let Some(spec) = spec else {
        return Ok(());
    };
    let cfg = ProcessIsolationConfig::from_env()?;
    ensure_can_apply(*spec, cfg.fs_enabled)?;

    #[cfg(target_os = "linux")]
    {
        let uid = spec.uid;
        let gid = spec.gid;
        let clear_groups = should_clear_groups_linux();
        let fs_view = filesystem_view_spec(uid, gid, cwd, home, None, &cfg)?;
        unsafe {
            cmd.pre_exec(move || {
                if let Some(fs_view) = fs_view.as_ref() {
                    setup_filesystem_view_linux(fs_view)
                        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
                }
                apply_current_process_linux(uid, gid, clear_groups)
            });
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = cmd;
        let _ = cwd;
        let _ = home;
        Err("OS 鐢ㄦ埛绾ц繘绋嬮殧绂讳粎鏀寔 Linux".to_string())
    }
}

pub(crate) fn terminal_helper_command(
    shell: &str,
    spec: Option<&ProcessIsolationSpec>,
    cwd: Option<&Path>,
    home: Option<&Path>,
) -> Result<(OsString, Vec<OsString>), String> {
    let Some(spec) = spec else {
        return Ok((OsString::from(shell), Vec::new()));
    };

    let exe = env::current_exe()
        .map_err(|err| format!("resolve process isolation helper failed: {err}"))?
        .into_os_string();
    let mut args = vec![
        OsString::from(EXEC_HELPER_ARG),
        OsString::from("--uid"),
        OsString::from(spec.uid.to_string()),
        OsString::from("--gid"),
        OsString::from(spec.gid.to_string()),
    ];
    if let Some(cwd) = cwd {
        args.push(OsString::from("--cwd"));
        args.push(cwd.as_os_str().to_os_string());
    }
    if let Some(home) = home {
        args.push(OsString::from("--home"));
        args.push(home.as_os_str().to_os_string());
    }
    args.extend([
        OsString::from("--terminal-shell-runtime"),
        OsString::from("--"),
        OsString::from(shell),
    ]);
    Ok((exe, args))
}

pub fn maybe_run_exec_helper_from_env() -> Result<bool, String> {
    let mut args = env::args_os();
    let _exe = args.next();
    let Some(first) = args.next() else {
        return Ok(false);
    };
    if first != OsStr::new(EXEC_HELPER_ARG) {
        return Ok(false);
    }

    #[cfg(target_os = "linux")]
    {
        run_exec_helper(args.collect())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        Err("OS 鐢ㄦ埛绾ц繘绋嬮殧绂?helper 浠呮敮鎸?Linux".to_string())
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
            "CHATOS_PROCESS_ISOLATION_{}_SPAN 蹇呴』澶т簬 0",
            label.to_ascii_uppercase()
        ));
    }
    if base == 0 {
        return Err(format!(
            "CHATOS_PROCESS_ISOLATION_{}_BASE 涓嶈兘涓?0",
            label.to_ascii_uppercase()
        ));
    }
    let max = u64::from(base) + u64::from(span) - 1;
    if max > u64::from(u32::MAX) {
        return Err(format!(
            "CHATOS_PROCESS_ISOLATION_{}_BASE/SPAN 瓒呭嚭 u32 鑼冨洿",
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
            .map_err(|err| format!("{key} 蹇呴』鏄?u32: {err}")),
        _ => Ok(default),
    }
}

fn env_usize(key: &str, default: usize) -> Result<usize, String> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<usize>()
            .map_err(|err| format!("{key} 蹇呴』鏄?usize: {err}")),
        _ => Ok(default),
    }
}

#[cfg(target_os = "linux")]
fn env_path(key: &str, default: &str) -> PathBuf {
    env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

#[cfg(target_os = "linux")]
fn ensure_can_apply(spec: ProcessIsolationSpec, fs_enabled: bool) -> Result<(), String> {
    let euid = unsafe { libc::geteuid() };
    let egid = unsafe { libc::getegid() };
    let can_set_identity =
        euid == 0 || (has_effective_cap(CAP_SETUID) && has_effective_cap(CAP_SETGID));
    if !can_set_identity && (euid != spec.uid || egid != spec.gid) {
        return Err(format!(
            "OS 鐢ㄦ埛绾ц繘绋嬮殧绂婚渶瑕佹湇鍔¤繘绋嬩互 root 杩愯锛屾垨鍏峰 CAP_SETUID/CAP_SETGID锛涘綋鍓?uid/gid={euid}/{egid}, 鐩爣 uid/gid={}/{}",
            spec.uid, spec.gid
        ));
    }
    if fs_enabled && euid != 0 && !has_effective_cap(CAP_SYS_ADMIN) {
        return Err(format!(
            "filesystem view isolation requires root or CAP_SYS_ADMIN; current uid/gid={euid}/{egid}, target uid/gid={}/{}",
            spec.uid, spec.gid
        ));
    }
    if fs_enabled && euid != 0 && !has_effective_cap(CAP_DAC_READ_SEARCH) {
        return Err(format!(
            "filesystem view isolation requires root or CAP_DAC_READ_SEARCH to resolve private workspace directories before dropping privileges; current uid/gid={euid}/{egid}, target uid/gid={}/{}",
            spec.uid, spec.gid
        ));
    }
    Ok(())
}

pub(crate) fn helper_env_vars() -> Vec<(&'static str, OsString)> {
    HELPER_ENV_KEYS
        .iter()
        .filter_map(|key| env::var_os(key).map(|value| (*key, value)))
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn ensure_can_apply(_spec: ProcessIsolationSpec, _fs_enabled: bool) -> Result<(), String> {
    Err("OS 鐢ㄦ埛绾ц繘绋嬮殧绂讳粎鏀寔 Linux".to_string())
}

#[cfg(target_os = "linux")]
fn chown_tree(path: &Path, uid: u32, gid: u32, max_entries: usize) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut count = 0usize;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|err| err.to_string())?;
        let metadata = std::fs::symlink_metadata(entry.path()).map_err(|err| err.to_string())?;
        if metadata.uid() == uid && metadata.gid() == gid {
            if metadata.file_type().is_dir() {
                ensure_can_set_private_permissions()?;
                std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o700))
                    .map_err(|err| err.to_string())?;
            }
            continue;
        }
        count = count.saturating_add(1);
        if count > max_entries {
            return Err(format!(
                "OS user process isolation chown exceeded limit: changed_entries>{max_entries}"
            ));
        }
        if unsafe { libc::geteuid() } != 0 && !has_effective_cap(CAP_CHOWN) {
            return Err(
                "OS 鐢ㄦ埛绾ц繘绋嬮殧绂婚渶瑕?root 鎴?CAP_CHOWN 鎵嶈兘鍑嗗宸ヤ綔鐩綍灞炰富"
                    .to_string(),
            );
        }
        lchown_path(entry.path(), uid, gid)?;
        if metadata.file_type().is_dir() {
            ensure_can_set_private_permissions()?;
            std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o700))
                .map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn path_owner_matches(path: &Path, uid: u32, gid: u32) -> Result<bool, String> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::symlink_metadata(path).map_err(|err| err.to_string())?;
    Ok(metadata.uid() == uid && metadata.gid() == gid)
}

#[cfg(target_os = "linux")]
fn prepare_user_scope_for_user(path: &Path, uid: u32, gid: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let Some(user_root) = find_user_scope_root(path) else {
        return Ok(());
    };
    for dir in [
        user_root.clone(),
        user_root.join("workspaces"),
        user_root.join("public"),
    ] {
        let Ok(metadata) = std::fs::symlink_metadata(dir.as_path()) else {
            continue;
        };
        if !metadata.file_type().is_dir() {
            continue;
        }
        if unsafe { libc::geteuid() } != 0 && !has_effective_cap(CAP_CHOWN) {
            return Err(
                "OS 鐢ㄦ埛绾ц繘绋嬮殧绂婚渶瑕?root 鎴?CAP_CHOWN 鎵嶈兘鍑嗗鐢ㄦ埛鐩綍灞炰富"
                    .to_string(),
            );
        }
        lchown_path(dir.as_path(), uid, gid)?;
        ensure_can_set_private_permissions()?;
        std::fs::set_permissions(dir.as_path(), std::fs::Permissions::from_mode(0o700))
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn find_user_scope_root(path: &Path) -> Option<std::path::PathBuf> {
    let mut out = std::path::PathBuf::new();
    let mut previous_was_users = false;
    for component in path.components() {
        out.push(component.as_os_str());
        if previous_was_users {
            return Some(out);
        }
        previous_was_users = component.as_os_str() == "users";
    }
    None
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
fn ensure_can_set_private_permissions() -> Result<(), String> {
    if unsafe { libc::geteuid() } == 0 || has_effective_cap(CAP_FOWNER) {
        Ok(())
    } else {
        Err("OS user process isolation requires root or CAP_FOWNER to set private workspace directory permissions".to_string())
    }
}

#[cfg(target_os = "linux")]
fn apply_current_process_linux(uid: u32, gid: u32, clear_groups: bool) -> std::io::Result<()> {
    unsafe {
        libc::umask(0o077);
        let _ = libc::prctl(libc::PR_SET_KEEPCAPS, 0, 0, 0, 0);
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
        let _ = libc::prctl(libc::PR_SET_KEEPCAPS, 0, 0, 0, 0);
        let _ = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
    }
    clear_capabilities_linux()?;
    Ok(())
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxCapHeader {
    version: u32,
    pid: i32,
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxCapData {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}

#[cfg(target_os = "linux")]
fn clear_capabilities_linux() -> std::io::Result<()> {
    let mut header = LinuxCapHeader {
        version: LINUX_CAPABILITY_VERSION_3,
        pid: 0,
    };
    let mut data = [LinuxCapData {
        effective: 0,
        permitted: 0,
        inheritable: 0,
    }; 2];
    let rc = unsafe {
        libc::syscall(
            libc::SYS_capset,
            &mut header as *mut LinuxCapHeader,
            data.as_mut_ptr(),
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
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

#[cfg(target_os = "linux")]
fn filesystem_view_spec(
    uid: u32,
    gid: u32,
    cwd: Option<&Path>,
    home: Option<&Path>,
    terminal_shell: Option<&Path>,
    cfg: &ProcessIsolationConfig,
) -> Result<Option<FilesystemViewSpec>, String> {
    if !cfg.fs_enabled {
        return Ok(None);
    }
    Ok(Some(FilesystemViewSpec {
        uid,
        gid,
        root: fs_view_root_for(&cfg.fs_root, uid, gid),
        cwd: cwd.map(Path::to_path_buf),
        home: home.map(Path::to_path_buf),
        terminal_shell: terminal_shell.map(Path::to_path_buf),
        mount_proc: cfg.fs_mount_proc,
    }))
}

#[cfg(target_os = "linux")]
fn fs_view_root_for(base: &Path, uid: u32, gid: u32) -> PathBuf {
    let service_uid = unsafe { libc::geteuid() };
    base.join(format!("svc-{service_uid}"))
        .join("roots")
        .join(format!("u{uid}-g{gid}"))
}

#[cfg(target_os = "linux")]
fn setup_filesystem_view_linux(view: &FilesystemViewSpec) -> Result<(), String> {
    prepare_filesystem_view_root(view)?;
    unshare_mount_namespace()?;
    make_mounts_private()?;
    bind_mount_dir(view.root.as_path(), view.root.as_path(), false)?;

    if let Some(cwd) = view.cwd.as_deref() {
        bind_mount_dir(
            cwd,
            view.root
                .join(relative_guest_path(GUEST_WORKSPACE)?)
                .as_path(),
            false,
        )?;
    }
    if let Some(home) = view.home.as_deref() {
        bind_mount_dir(
            home,
            view.root.join(relative_guest_path(GUEST_HOME)?).as_path(),
            false,
        )?;
    }

    bind_minimal_dev_files(view.root.as_path())?;
    bind_basic_command_runtime(view.root.as_path())?;
    if let Some(shell) = view.terminal_shell.as_deref() {
        bind_terminal_shell_runtime(view.root.as_path(), shell)?;
    }
    if view.mount_proc {
        mount_proc(view.root.join("proc").as_path())?;
    }

    pivot_into_filesystem_view(view.root.as_path())?;
    chdir_guest(if view.cwd.is_some() {
        GUEST_WORKSPACE
    } else {
        GUEST_HOME
    })?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn prepare_filesystem_view_root(view: &FilesystemViewSpec) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let service_uid = unsafe { libc::geteuid() };
    let configured_base = view
        .root
        .ancestors()
        .nth(3)
        .ok_or_else(|| format!("invalid filesystem view root: {}", view.root.display()))?;
    let service_base = view
        .root
        .ancestors()
        .nth(2)
        .ok_or_else(|| format!("invalid filesystem view root: {}", view.root.display()))?;
    ensure_owned_dir(configured_base, 0o700, service_uid)?;
    ensure_owned_dir(service_base, 0o700, service_uid)?;
    ensure_owned_dir(service_base.join("roots").as_path(), 0o700, service_uid)?;
    ensure_owned_dir(view.root.as_path(), 0o755, service_uid)?;

    for dir in [
        "workspace",
        "home",
        "home/chatos",
        "tmp",
        "dev",
        "proc",
        "etc",
    ] {
        std::fs::create_dir_all(view.root.join(dir)).map_err(|err| err.to_string())?;
    }
    std::fs::set_permissions(
        view.root.join("tmp"),
        std::fs::Permissions::from_mode(0o1777),
    )
    .map_err(|err| err.to_string())?;
    if view.home.is_none() {
        lchown_path(view.root.join("home/chatos").as_path(), view.uid, view.gid)?;
        std::fs::set_permissions(
            view.root.join("home/chatos"),
            std::fs::Permissions::from_mode(0o700),
        )
        .map_err(|err| err.to_string())?;
    }
    write_identity_files(view)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn ensure_owned_dir(path: &Path, mode: u32, owner_uid: u32) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    std::fs::create_dir_all(path)
        .map_err(|err| format!("create {} failed: {err}", path.display()))?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|err| format!("stat {} failed: {err}", path.display()))?;
    if !metadata.file_type().is_dir() {
        return Err(format!("{} must be a directory", path.display()));
    }
    if metadata.uid() != owner_uid {
        return Err(format!(
            "{} must be owned by uid {owner_uid}, current owner is {}",
            path.display(),
            metadata.uid()
        ));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|err| format!("chmod {} failed: {err}", path.display()))
}

#[cfg(target_os = "linux")]
fn write_identity_files(view: &FilesystemViewSpec) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let shell = view
        .terminal_shell
        .as_deref()
        .and_then(Path::to_str)
        .unwrap_or("/bin/sh");
    let user = format!("chatos-u{}", view.uid);
    let passwd = format!(
        "root:x:0:0:root:/root:/usr/sbin/nologin\n{user}:x:{}:{}:chatos isolated user:{GUEST_HOME}:{shell}\n",
        view.uid, view.gid
    );
    let group = format!("root:x:0:\n{user}:x:{}:\n", view.gid);
    let passwd_path = view.root.join("etc/passwd");
    let group_path = view.root.join("etc/group");
    std::fs::write(passwd_path.as_path(), passwd).map_err(|err| err.to_string())?;
    std::fs::write(group_path.as_path(), group).map_err(|err| err.to_string())?;
    std::fs::set_permissions(passwd_path, std::fs::Permissions::from_mode(0o644))
        .map_err(|err| err.to_string())?;
    std::fs::set_permissions(group_path, std::fs::Permissions::from_mode(0o644))
        .map_err(|err| err.to_string())
}

#[cfg(target_os = "linux")]
fn bind_terminal_shell_runtime(root: &Path, shell: &Path) -> Result<(), String> {
    bind_executable_runtime(root, shell)
}

#[cfg(target_os = "linux")]
fn bind_basic_command_runtime(root: &Path) -> Result<(), String> {
    use std::collections::BTreeSet;

    const SEARCH_DIRS: &[&str] = &[
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/local/sbin",
        "/usr/sbin",
        "/sbin",
    ];
    const COMMANDS: &[&str] = &[
        "sh",
        "bash",
        "env",
        "ls",
        "mkdir",
        "rmdir",
        "touch",
        "cat",
        "cp",
        "mv",
        "rm",
        "ln",
        "chmod",
        "stat",
        "readlink",
        "realpath",
        "dirname",
        "basename",
        "find",
        "xargs",
        "grep",
        "egrep",
        "fgrep",
        "sed",
        "awk",
        "sort",
        "uniq",
        "wc",
        "head",
        "tail",
        "cut",
        "tr",
        "tee",
        "printf",
        "date",
        "sleep",
        "true",
        "false",
        "test",
        "id",
        "whoami",
        "groups",
        "uname",
        "du",
        "df",
        "tar",
        "gzip",
        "gunzip",
        "zcat",
        "bzip2",
        "bunzip2",
        "xz",
        "unxz",
        "zip",
        "unzip",
        "curl",
        "wget",
        "sha256sum",
        "sha1sum",
        "md5sum",
        "base64",
        "file",
        "less",
        "more",
        "clear",
        "stty",
        "tput",
    ];

    let mut bound = BTreeSet::new();
    for command in COMMANDS {
        for dir in SEARCH_DIRS {
            let candidate = Path::new(dir).join(command);
            if !candidate.exists() {
                continue;
            }
            if !std::fs::metadata(candidate.as_path())
                .map(|metadata| metadata.is_file())
                .unwrap_or(false)
            {
                continue;
            }
            if bound.insert(candidate.clone()) {
                bind_executable_runtime(root, candidate.as_path())?;
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn bind_executable_runtime(root: &Path, executable: &Path) -> Result<(), String> {
    let guest_path = absolute_guest_path(executable)?;
    bind_mount_file_readonly(
        executable,
        root.join(relative_guest_path(&guest_path)?).as_path(),
    )?;

    for dep in shared_library_dependencies(executable)? {
        bind_mount_file_readonly(
            &dep,
            root.join(relative_guest_path(dep.as_path())?).as_path(),
        )?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn shared_library_dependencies(binary: &Path) -> Result<Vec<PathBuf>, String> {
    use std::collections::BTreeSet;

    let output = run_ldd(binary)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(output.stderr.as_slice());
        if stderr.contains("not a dynamic executable") {
            return Ok(Vec::new());
        }
        return Err(format!(
            "ldd {} failed: {}",
            binary.display(),
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(output.stdout.as_slice());
    let mut deps = BTreeSet::new();
    for line in stdout.lines() {
        if let Some(path) = parse_ldd_path(line) {
            deps.insert(path);
        }
    }
    Ok(deps.into_iter().collect())
}

#[cfg(target_os = "linux")]
fn run_ldd(binary: &Path) -> Result<std::process::Output, String> {
    for candidate in ["/usr/bin/ldd", "/bin/ldd", "ldd"] {
        match std::process::Command::new(candidate).arg(binary).output() {
            Ok(output) => return Ok(output),
            Err(_) if candidate != "ldd" => continue,
            Err(err) => return Err(format!("run ldd failed: {err}")),
        }
    }
    Err("run ldd failed".to_string())
}

#[cfg(target_os = "linux")]
fn parse_ldd_path(line: &str) -> Option<PathBuf> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("linux-vdso") {
        return None;
    }
    if let Some((_, right)) = trimmed.split_once("=>") {
        let path = right.trim().split_whitespace().next()?;
        if path.starts_with('/') {
            return Some(PathBuf::from(path));
        }
        return None;
    }
    let first = trimmed.split_whitespace().next()?;
    first.starts_with('/').then(|| PathBuf::from(first))
}

#[cfg(target_os = "linux")]
fn bind_minimal_dev_files(root: &Path) -> Result<(), String> {
    for device in ["/dev/null", "/dev/zero", "/dev/random", "/dev/urandom"] {
        let source = Path::new(device);
        if source.exists() {
            bind_mount_device_file(source, root.join(relative_guest_path(device)?).as_path())?;
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn bind_mount_dir(source: &Path, target: &Path, allow_devices: bool) -> Result<(), String> {
    let source = std::fs::canonicalize(source)
        .map_err(|err| format!("resolve bind source {} failed: {err}", source.display()))?;
    let metadata = std::fs::metadata(source.as_path())
        .map_err(|err| format!("stat bind source {} failed: {err}", source.display()))?;
    if !metadata.is_dir() {
        return Err(format!(
            "bind source {} must be a directory",
            source.display()
        ));
    }
    std::fs::create_dir_all(target)
        .map_err(|err| format!("create bind target {} failed: {err}", target.display()))?;
    mount_bind(source.as_path(), target, false, allow_devices)
}

#[cfg(target_os = "linux")]
fn bind_mount_file_readonly(source: &Path, target: &Path) -> Result<(), String> {
    bind_mount_file(source, target, true)
}

#[cfg(target_os = "linux")]
fn bind_mount_file(source: &Path, target: &Path, readonly: bool) -> Result<(), String> {
    let source = std::fs::canonicalize(source)
        .map_err(|err| format!("resolve bind source {} failed: {err}", source.display()))?;
    let metadata = std::fs::metadata(source.as_path())
        .map_err(|err| format!("stat bind source {} failed: {err}", source.display()))?;
    if !metadata.is_file() {
        return Err(format!("bind source {} must be a file", source.display()));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "create bind target parent {} failed: {err}",
                parent.display()
            )
        })?;
    }
    ensure_regular_file(target)?;
    mount_bind(source.as_path(), target, readonly, false)
}

#[cfg(target_os = "linux")]
fn bind_mount_device_file(source: &Path, target: &Path) -> Result<(), String> {
    let source = std::fs::canonicalize(source)
        .map_err(|err| format!("resolve bind source {} failed: {err}", source.display()))?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "create bind target parent {} failed: {err}",
                parent.display()
            )
        })?;
    }
    ensure_regular_file(target)?;
    mount_bind(source.as_path(), target, false, true)
}

#[cfg(target_os = "linux")]
fn ensure_regular_file(path: &Path) -> Result<(), String> {
    use std::fs::OpenOptions;

    if path.exists() {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|err| format!("stat {} failed: {err}", path.display()))?;
        if !metadata.file_type().is_file() {
            return Err(format!("{} must be a regular file", path.display()));
        }
        return Ok(());
    }
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map(|_| ())
        .map_err(|err| format!("create file {} failed: {err}", path.display()))
}

#[cfg(target_os = "linux")]
fn mount_bind(
    source: &Path,
    target: &Path,
    readonly: bool,
    allow_devices: bool,
) -> Result<(), String> {
    let source_c = cstring_path(source)?;
    let target_c = cstring_path(target)?;
    let bind_flags = (libc::MS_BIND | libc::MS_REC) as libc::c_ulong;
    let rc = unsafe {
        libc::mount(
            source_c.as_ptr(),
            target_c.as_ptr(),
            std::ptr::null(),
            bind_flags,
            std::ptr::null(),
        )
    };
    if rc != 0 {
        return Err(format!(
            "bind mount {} -> {} failed: {}",
            source.display(),
            target.display(),
            std::io::Error::last_os_error()
        ));
    }

    let mut remount_flags = libc::MS_BIND | libc::MS_REMOUNT | libc::MS_NOSUID;
    if readonly {
        remount_flags |= libc::MS_RDONLY;
    }
    if !allow_devices {
        remount_flags |= libc::MS_NODEV;
    }
    let rc = unsafe {
        libc::mount(
            std::ptr::null(),
            target_c.as_ptr(),
            std::ptr::null(),
            remount_flags as libc::c_ulong,
            std::ptr::null(),
        )
    };
    if rc != 0 {
        return Err(format!(
            "remount {} failed: {}",
            target.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn mount_proc(target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|err| err.to_string())?;
    let source = std::ffi::CString::new("proc").expect("static string");
    let fstype = std::ffi::CString::new("proc").expect("static string");
    let target_c = cstring_path(target)?;
    let flags = (libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC) as libc::c_ulong;
    let rc = unsafe {
        libc::mount(
            source.as_ptr(),
            target_c.as_ptr(),
            fstype.as_ptr(),
            flags,
            std::ptr::null(),
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "mount proc at {} failed: {}",
            target.display(),
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(target_os = "linux")]
fn unshare_mount_namespace() -> Result<(), String> {
    let rc = unsafe { libc::unshare(libc::CLONE_NEWNS) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "unshare mount namespace failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(target_os = "linux")]
fn make_mounts_private() -> Result<(), String> {
    let root = std::ffi::CString::new("/").expect("static string");
    let flags = (libc::MS_REC | libc::MS_PRIVATE) as libc::c_ulong;
    let rc = unsafe {
        libc::mount(
            std::ptr::null(),
            root.as_ptr(),
            std::ptr::null(),
            flags,
            std::ptr::null(),
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "make mount namespace private failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(target_os = "linux")]
fn pivot_into_filesystem_view(root: &Path) -> Result<(), String> {
    let old_root = root.join(PIVOT_OLD_ROOT);
    std::fs::create_dir_all(old_root.as_path()).map_err(|err| err.to_string())?;
    let root_c = cstring_path(root)?;
    let old_root_c = cstring_path(old_root.as_path())?;
    let rc = unsafe { libc::syscall(libc::SYS_pivot_root, root_c.as_ptr(), old_root_c.as_ptr()) };
    if rc != 0 {
        return Err(format!(
            "pivot_root into {} failed: {}",
            root.display(),
            std::io::Error::last_os_error()
        ));
    }
    chdir_guest("/")?;
    let old_root_guest = format!("/{PIVOT_OLD_ROOT}");
    let old_root_guest_c = std::ffi::CString::new(old_root_guest.as_str()).expect("static string");
    let rc = unsafe { libc::umount2(old_root_guest_c.as_ptr(), libc::MNT_DETACH) };
    if rc != 0 {
        return Err(format!(
            "unmount old root failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let rc = unsafe { libc::rmdir(old_root_guest_c.as_ptr()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "remove old root mount point failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(target_os = "linux")]
fn chdir_guest(path: &str) -> Result<(), String> {
    let c_path = std::ffi::CString::new(path).map_err(|_| format!("path contains NUL: {path}"))?;
    let rc = unsafe { libc::chdir(c_path.as_ptr()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "chdir {path} failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(target_os = "linux")]
fn relative_guest_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, String> {
    let path = path.as_ref();
    path.strip_prefix("/")
        .map(Path::to_path_buf)
        .map_err(|_| format!("guest path must be absolute: {}", path.display()))
}

#[cfg(target_os = "linux")]
fn absolute_guest_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Err(format!("guest path must be absolute: {}", path.display()))
    }
}

#[cfg(target_os = "linux")]
fn cstring_path(path: &Path) -> Result<std::ffi::CString, String> {
    use std::os::unix::ffi::OsStrExt;

    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("path contains NUL byte: {}", path.display()))
}

#[cfg(target_os = "linux")]
fn run_exec_helper(args: Vec<OsString>) -> Result<bool, String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let mut index = 0usize;
    let uid = parse_flag_u32(args.as_slice(), &mut index, "--uid")?;
    let gid = parse_flag_u32(args.as_slice(), &mut index, "--gid")?;
    let mut cwd: Option<PathBuf> = None;
    let mut home: Option<PathBuf> = None;
    let mut terminal_shell_runtime = false;
    while args.get(index).map(OsString::as_os_str) != Some(OsStr::new("--")) {
        let Some(flag) = args.get(index).map(OsString::as_os_str) else {
            return Err("process isolation helper missing -- separator".to_string());
        };
        if flag == OsStr::new("--cwd") {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| "process isolation helper missing value for --cwd".to_string())?;
            cwd = Some(PathBuf::from(value.as_os_str()));
            index += 1;
            continue;
        }
        if flag == OsStr::new("--home") {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| "process isolation helper missing value for --home".to_string())?;
            home = Some(PathBuf::from(value.as_os_str()));
            index += 1;
            continue;
        }
        if flag == OsStr::new("--terminal-shell-runtime") {
            terminal_shell_runtime = true;
            index += 1;
            continue;
        }
        return Err(format!(
            "process isolation helper unknown flag: {}",
            flag.to_string_lossy()
        ));
    }
    if args.get(index).map(OsString::as_os_str) != Some(OsStr::new("--")) {
        return Err("process isolation helper missing -- separator".to_string());
    }
    index += 1;
    let command = args
        .get(index)
        .ok_or_else(|| "process isolation helper missing target command".to_string())?;
    let command_args = &args[index..];

    let cfg = ProcessIsolationConfig::from_env()?;
    ensure_can_apply(ProcessIsolationSpec { uid, gid }, cfg.fs_enabled)?;
    let clear_groups = should_clear_groups_linux();
    let terminal_shell = terminal_shell_runtime.then(|| Path::new(command).to_path_buf());
    if let Some(fs_view) = filesystem_view_spec(
        uid,
        gid,
        cwd.as_deref(),
        home.as_deref(),
        terminal_shell.as_deref(),
        &cfg,
    )? {
        setup_filesystem_view_linux(&fs_view)?;
    } else if let Some(cwd) = cwd.as_deref() {
        std::env::set_current_dir(cwd).map_err(|err| {
            format!(
                "process isolation helper chdir {} failed: {err}",
                cwd.display()
            )
        })?;
    }

    apply_current_process_linux(uid, gid, clear_groups)
        .map_err(|err| format!("process isolation privilege drop failed: {err}"))?;

    let mut cstrings = Vec::with_capacity(command_args.len());
    for arg in command_args {
        cstrings.push(
            CString::new(arg.as_os_str().as_bytes())
                .map_err(|_| "process isolation helper argument contains NUL byte".to_string())?,
        );
    }
    let mut argv = cstrings
        .iter()
        .map(|arg| arg.as_ptr())
        .collect::<Vec<*const libc::c_char>>();
    argv.push(std::ptr::null());
    let program = CString::new(command.as_os_str().as_bytes())
        .map_err(|_| "process isolation helper command contains NUL byte".to_string())?;

    unsafe {
        libc::execvp(program.as_ptr(), argv.as_ptr());
    }
    Err(format!(
        "process isolation helper exec failed: {}",
        std::io::Error::last_os_error()
    ))
}

#[cfg(target_os = "linux")]
fn parse_flag_u32(args: &[OsString], index: &mut usize, flag: &str) -> Result<u32, String> {
    if args.get(*index).map(OsString::as_os_str) != Some(OsStr::new(flag)) {
        return Err(format!("process isolation helper missing {flag}"));
    }
    *index += 1;
    let value = args
        .get(*index)
        .ok_or_else(|| format!("process isolation helper missing value for {flag}"))?;
    *index += 1;
    value
        .to_string_lossy()
        .parse::<u32>()
        .map_err(|err| format!("process isolation helper invalid {flag}: {err}"))
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
