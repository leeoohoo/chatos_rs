use super::plugin_runtime_error;
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

const MAXIMUM_PLUGIN_SKILLS: usize = 128;
const MAXIMUM_SKILL_INSTRUCTIONS_BYTES: u64 = 256 * 1_024;
const MAXIMUM_SKILL_RESOURCE_BYTES: u64 = 1_024 * 1_024;
const MAXIMUM_SKILL_RESOURCES: usize = 256;
const MAXIMUM_SKILL_DIRECTORY_ENTRIES: usize = MAXIMUM_SKILL_RESOURCES * 2;
const MAXIMUM_SKILL_CATALOG_BYTES: u64 = 16 * 1_024 * 1_024;

#[derive(Clone, Debug)]
pub(crate) struct AgentPluginSkillSnapshot {
    pub(super) documents: BTreeMap<String, AgentPluginSkillDocument>,
}

impl AgentPluginSkillSnapshot {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.documents.len()
    }
}

#[derive(Clone, Debug)]
pub(super) struct AgentPluginSkillDocument {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) role: String,
    pub(super) instructions: String,
    pub(super) resources: BTreeMap<String, String>,
}

#[derive(Debug)]
pub(super) struct SkillResourcePage {
    pub(super) content: String,
    pub(super) offset: usize,
    pub(super) next_offset: Option<usize>,
    pub(super) truncated: bool,
}

pub(crate) fn load_agent_plugin_skill_snapshot<'a>(
    installation_root: &Path,
    package_name: &str,
    manifest_directories: impl Iterator<Item = &'a str>,
) -> Result<AgentPluginSkillSnapshot, String> {
    let installation_root = installation_root
        .canonicalize()
        .map_err(|_| plugin_runtime_error())?;
    if !installation_root.is_dir() {
        return Err(plugin_runtime_error());
    }
    let mut directories = manifest_directories
        .take(MAXIMUM_PLUGIN_SKILLS.saturating_add(1))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if directories.is_empty() {
        directories = discover_bundled_skill_directories(&installation_root, package_name)?;
    }
    if directories.len() > MAXIMUM_PLUGIN_SKILLS {
        return Err(plugin_runtime_error());
    }
    let mut documents = BTreeMap::new();
    let mut total_bytes = 0_u64;
    let mut observed_directories = HashSet::new();
    for raw_directory in directories {
        let relative = normalized_skill_relative_path(&raw_directory)?;
        if !observed_directories.insert(relative.clone()) {
            return Err(plugin_runtime_error());
        }
        let directory = secure_existing_skill_path(&installation_root, &relative)?;
        if !directory.is_dir() {
            return Err(plugin_runtime_error());
        }
        let expected_name = directory
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(plugin_runtime_error)?;
        let skill_path =
            secure_existing_skill_path(&installation_root, &relative.join("SKILL.md"))?;
        let skill_bytes = read_bounded_skill_file(
            &installation_root,
            &skill_path,
            MAXIMUM_SKILL_INSTRUCTIONS_BYTES,
        )?;
        total_bytes = total_bytes.saturating_add(skill_bytes.len() as u64);
        if total_bytes > MAXIMUM_SKILL_CATALOG_BYTES {
            return Err(plugin_runtime_error());
        }
        let raw = String::from_utf8(skill_bytes).map_err(|_| plugin_runtime_error())?;
        let (name, description, role, instructions) =
            parse_plugin_skill_document(&raw, expected_name)?;
        let resources = load_skill_resources(&installation_root, &directory, &mut total_bytes)?;
        let document = AgentPluginSkillDocument {
            name: name.clone(),
            description,
            role,
            instructions,
            resources,
        };
        if documents.insert(name, document).is_some() {
            return Err(plugin_runtime_error());
        }
    }
    Ok(AgentPluginSkillSnapshot { documents })
}

fn discover_bundled_skill_directories(
    installation_root: &Path,
    package_name: &str,
) -> Result<Vec<String>, String> {
    let mut roots = vec![("skills".to_owned(), PathBuf::from("skills"))];
    if !package_name.trim().is_empty() {
        let package = normalized_skill_relative_path(package_name)?;
        roots.push((
            format!("node_modules/{}/skills", path_to_slashes(&package)?),
            PathBuf::from("node_modules").join(package).join("skills"),
        ));
    }
    let mut result = Vec::new();
    for (relative_root, root_path) in roots {
        let candidate = installation_root.join(&root_path);
        let metadata = match std::fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(plugin_runtime_error()),
        };
        if !metadata.is_dir() || is_symlink_or_reparse(&metadata) {
            continue;
        }
        let root = secure_existing_skill_path(installation_root, &root_path)?;
        for (visited, child) in std::fs::read_dir(root)
            .map_err(|_| plugin_runtime_error())?
            .enumerate()
        {
            if visited >= MAXIMUM_SKILL_DIRECTORY_ENTRIES {
                return Err(plugin_runtime_error());
            }
            let child = child.map_err(|_| plugin_runtime_error())?;
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| plugin_runtime_error())?;
            if name.starts_with('.') || !valid_plugin_skill_name(&name) {
                continue;
            }
            let metadata = child.metadata().map_err(|_| plugin_runtime_error())?;
            let link_metadata =
                std::fs::symlink_metadata(child.path()).map_err(|_| plugin_runtime_error())?;
            if !metadata.is_dir() || is_symlink_or_reparse(&link_metadata) {
                continue;
            }
            let skill = child.path().join("SKILL.md");
            let Ok(skill_metadata) = std::fs::symlink_metadata(&skill) else {
                continue;
            };
            if !skill_metadata.is_file() || is_symlink_or_reparse(&skill_metadata) {
                continue;
            }
            result.push(format!("{relative_root}/{name}"));
            if result.len() > 64 {
                return Err(plugin_runtime_error());
            }
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

pub(super) fn normalized_skill_relative_path(raw: &str) -> Result<PathBuf, String> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.len() > 4_096
        || raw.chars().any(char::is_control)
        || raw.contains('\\')
    {
        return Err(plugin_runtime_error());
    }
    let normalized = raw.strip_prefix("./").unwrap_or(raw);
    let mut path = PathBuf::new();
    for component in normalized.split('/') {
        if component.is_empty() || component == "." || component == ".." || component.contains(':')
        {
            return Err(plugin_runtime_error());
        }
        path.push(component);
    }
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(plugin_runtime_error());
    }
    Ok(path)
}

fn secure_existing_skill_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(plugin_runtime_error());
        };
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current).map_err(|_| plugin_runtime_error())?;
        if is_symlink_or_reparse(&metadata) {
            return Err(plugin_runtime_error());
        }
    }
    let canonical = current.canonicalize().map_err(|_| plugin_runtime_error())?;
    if !canonical.starts_with(root) {
        return Err(plugin_runtime_error());
    }
    Ok(canonical)
}

fn is_symlink_or_reparse(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn read_bounded_skill_file(root: &Path, path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    use std::io::Read as _;

    let relative = path
        .strip_prefix(root)
        .map_err(|_| plugin_runtime_error())?;
    let path = secure_existing_skill_path(root, relative)?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| plugin_runtime_error())?;
    if !metadata.is_file() || is_symlink_or_reparse(&metadata) || metadata.len() > maximum {
        return Err(plugin_runtime_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(path)
        .and_then(|file| file.take(maximum + 1).read_to_end(&mut bytes))
        .map_err(|_| plugin_runtime_error())?;
    if bytes.len() as u64 > maximum {
        return Err(plugin_runtime_error());
    }
    Ok(bytes)
}

fn parse_plugin_skill_document(
    raw: &str,
    expected_name: &str,
) -> Result<(String, String, String, String), String> {
    let normalized = raw.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix("---\n")
        .ok_or_else(plugin_runtime_error)?;
    let end = rest.find("\n---\n").ok_or_else(plugin_runtime_error)?;
    let frontmatter = &rest[..end];
    let name = skill_frontmatter_scalar("name", frontmatter).unwrap_or_default();
    let description = skill_frontmatter_scalar("description", frontmatter).unwrap_or_default();
    let role =
        skill_frontmatter_scalar("chatos.role", frontmatter).unwrap_or_else(|| "leaf".into());
    let body = rest[end + 5..].trim();
    if name != expected_name
        || !valid_plugin_skill_name(&name)
        || description.is_empty()
        || description.chars().count() > 16_000
        || body.is_empty()
        || !matches!(role.as_str(), "router" | "leaf")
    {
        return Err(plugin_runtime_error());
    }
    Ok((name, description, role, normalized.trim().to_owned()))
}

fn skill_frontmatter_scalar(key: &str, frontmatter: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let raw = frontmatter.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix(&prefix).map(str::trim)
    })?;
    let value = if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };
    (!value.is_empty()).then(|| value.to_owned())
}

fn valid_plugin_skill_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
}

fn load_skill_resources(
    installation_root: &Path,
    skill_directory: &Path,
    total_bytes: &mut u64,
) -> Result<BTreeMap<String, String>, String> {
    let mut resources = BTreeMap::new();
    let mut observed = 0_usize;
    let mut visited_entries = 0_usize;
    let mut stack = Vec::new();
    for resource_root in ["scripts", "references", "assets"] {
        let candidate = skill_directory.join(resource_root);
        let metadata = match std::fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(plugin_runtime_error()),
        };
        if !metadata.is_dir() || is_symlink_or_reparse(&metadata) {
            return Err(plugin_runtime_error());
        }
        let canonical = candidate
            .canonicalize()
            .map_err(|_| plugin_runtime_error())?;
        if !canonical.starts_with(installation_root) {
            return Err(plugin_runtime_error());
        }
        stack.push((canonical, PathBuf::from(resource_root)));
    }
    while let Some((directory, relative_directory)) = stack.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|_| plugin_runtime_error())? {
            visited_entries = visited_entries.saturating_add(1);
            if visited_entries > MAXIMUM_SKILL_DIRECTORY_ENTRIES {
                return Err(plugin_runtime_error());
            }
            let entry = entry.map_err(|_| plugin_runtime_error())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| plugin_runtime_error())?;
            if name.starts_with('.') {
                continue;
            }
            let metadata =
                std::fs::symlink_metadata(entry.path()).map_err(|_| plugin_runtime_error())?;
            if is_symlink_or_reparse(&metadata) {
                return Err(plugin_runtime_error());
            }
            let relative = relative_directory.join(&name);
            if metadata.is_dir() {
                stack.push((entry.path(), relative));
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            observed += 1;
            if observed > MAXIMUM_SKILL_RESOURCES {
                return Err(plugin_runtime_error());
            }
            let bytes = read_bounded_skill_file(
                installation_root,
                &entry.path(),
                MAXIMUM_SKILL_RESOURCE_BYTES,
            )?;
            *total_bytes = total_bytes.saturating_add(bytes.len() as u64);
            if *total_bytes > MAXIMUM_SKILL_CATALOG_BYTES {
                return Err(plugin_runtime_error());
            }
            let Ok(content) = String::from_utf8(bytes) else {
                continue;
            };
            resources.insert(path_to_slashes(&relative)?, content);
        }
    }
    Ok(resources)
}

pub(super) fn path_to_slashes(path: &Path) -> Result<String, String> {
    path.iter()
        .map(|component| {
            component
                .to_str()
                .map(str::to_owned)
                .ok_or_else(plugin_runtime_error)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| components.join("/"))
}
