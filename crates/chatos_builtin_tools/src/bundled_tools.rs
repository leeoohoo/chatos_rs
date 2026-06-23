use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const CHATOS_BUNDLED_TOOLS_DIR_ENV: &str = "CHATOS_BUNDLED_TOOLS_DIR";
pub const CHATOS_BUNDLED_TOOLS_PATH_ENV: &str = "CHATOS_BUNDLED_TOOLS_PATH";

const BUNDLED_TOOLS_DIR_NAME: &str = "bundled-tools";
const RIPGREP_BIN_NAME: &str = "rg";

pub fn bundled_tool_path(tool_name: &str) -> Option<PathBuf> {
    let bin_name = platform_tool_file_name(tool_name);
    discover_bundled_tool_dirs()
        .into_iter()
        .map(|dir| dir.join(&bin_name))
        .find(|path| path.is_file())
}

pub fn path_with_bundled_tools(base_path: Option<OsString>) -> Option<OsString> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for dir in discover_bundled_tool_dirs() {
        push_unique_path(&mut entries, &mut seen, dir);
    }

    if let Some(path) = base_path.as_ref() {
        for entry in env::split_paths(path) {
            push_unique_path(&mut entries, &mut seen, entry);
        }
    }

    if entries.is_empty() {
        return base_path;
    }

    env::join_paths(entries).ok().or(base_path)
}

pub fn discover_bundled_tool_dirs() -> Vec<PathBuf> {
    let platform_dir_name = platform_tools_dir_name();
    let mut seen = HashSet::new();
    let mut dirs = Vec::new();

    if let Some(path) = env::var_os(CHATOS_BUNDLED_TOOLS_PATH_ENV) {
        for dir in env::split_paths(&path) {
            push_tool_dir_if_ready(&mut dirs, &mut seen, dir);
        }
    }

    if let Some(root) = env::var_os(CHATOS_BUNDLED_TOOLS_DIR_ENV) {
        push_expanded_tool_dir(
            &mut dirs,
            &mut seen,
            PathBuf::from(root),
            &platform_dir_name,
        );
    }

    for root in candidate_tool_roots() {
        push_expanded_tool_dir(&mut dirs, &mut seen, root, &platform_dir_name);
    }

    dirs
}

fn candidate_tool_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            roots.push(exe_dir.join(BUNDLED_TOOLS_DIR_NAME));
            roots.push(exe_dir.join("tools"));
            if let Some(contents_dir) = exe_dir.parent() {
                roots.push(contents_dir.join(BUNDLED_TOOLS_DIR_NAME));
                roots.push(contents_dir.join("tools"));
                roots.push(contents_dir.join("Resources").join(BUNDLED_TOOLS_DIR_NAME));
                roots.push(contents_dir.join("Resources").join("tools"));
            }
        }
    }

    if let Ok(current_dir) = env::current_dir() {
        for ancestor in current_dir.ancestors() {
            roots.push(ancestor.join(BUNDLED_TOOLS_DIR_NAME));
        }
    }

    for ancestor in Path::new(env!("CARGO_MANIFEST_DIR")).ancestors() {
        roots.push(ancestor.join(BUNDLED_TOOLS_DIR_NAME));
    }

    roots
}

fn push_expanded_tool_dir(
    dirs: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    root_or_dir: PathBuf,
    platform_dir_name: &str,
) {
    push_tool_dir_if_ready(dirs, seen, root_or_dir.clone());
    push_tool_dir_if_ready(dirs, seen, root_or_dir.join(platform_dir_name));
}

fn push_tool_dir_if_ready(dirs: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, dir: PathBuf) {
    if !dir.is_dir()
        || !dir
            .join(platform_tool_file_name(RIPGREP_BIN_NAME))
            .is_file()
    {
        return;
    }
    push_unique_path(dirs, seen, dir);
}

fn push_unique_path(entries: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    if seen.insert(path.clone()) {
        entries.push(path);
    }
}

fn platform_tools_dir_name() -> String {
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64".to_string(),
        ("macos", "x86_64") => "macos-x64".to_string(),
        ("linux", "x86_64") => "linux-x64".to_string(),
        ("linux", "aarch64") => "linux-arm64".to_string(),
        ("windows", "x86_64") => "windows-x64".to_string(),
        ("windows", "aarch64") => "windows-arm64".to_string(),
        (os, arch) => format!("{os}-{arch}"),
    }
}

fn platform_tool_file_name(tool_name: &str) -> String {
    if cfg!(windows) && !tool_name.ends_with(".exe") {
        format!("{tool_name}.exe")
    } else {
        tool_name.to_string()
    }
}
