use super::*;

pub(super) fn normalize_compatible_skills(package_root: &Path) -> Result<Vec<String>, String> {
    let root_skill = package_root.join("SKILL.md");
    if root_skill.is_file() {
        let raw = read_bounded_text(&root_skill, 256 * 1_024)?;
        let name = compatible_root_skill_name(&raw)?;
        let target = package_root.join("skills").join(&name);
        if target.exists() {
            return Err(install_error(&format!(
                "根目录 SKILL.md 与 skills/{name} 冲突"
            )));
        }
        ensure_owned_directory(&target).map_err(|_| install_error("无法创建兼容 Skill 目录"))?;
        fs::copy(&root_skill, target.join("SKILL.md"))
            .map_err(|_| install_error("无法复制兼容 SKILL.md"))?;
        for resource in ["scripts", "references", "assets"] {
            let source = package_root.join(resource);
            if source.exists() {
                copy_compatible_skill_tree(&source, &target.join(resource))?;
            }
        }
    }

    let skills_root = package_root.join("skills");
    let metadata = match fs::symlink_metadata(&skills_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(install_error("无法读取兼容 Skill 目录")),
    };
    if !metadata.is_dir() || is_symlink_or_reparse(&metadata) {
        return Err(install_error("兼容 Skill 根目录无效"));
    }
    let mut skills = Vec::new();
    for (visited, entry) in fs::read_dir(&skills_root)
        .map_err(|_| install_error("无法读取兼容 Skill 目录"))?
        .enumerate()
    {
        if visited >= MAXIMUM_COMPATIBLE_SKILL_ENTRIES {
            return Err(install_error("兼容 Skill 目录条目超过限制"));
        }
        let entry = entry.map_err(|_| install_error("无法读取兼容 Skill 目录"))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| install_error("兼容 Skill 名称无效"))?;
        if name.starts_with('.') {
            continue;
        }
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| install_error("无法读取兼容 Skill"))?;
        if is_symlink_or_reparse(&metadata) {
            return Err(install_error("兼容 Skill 不允许链接或重解析点"));
        }
        if !metadata.is_dir() || !entry.path().join("SKILL.md").is_file() {
            continue;
        }
        if !valid_compatible_skill_name(&name) {
            return Err(install_error("兼容 Skill 目录名无效"));
        }
        skills.push(format!("skills/{name}"));
        if skills.len() > 128 {
            return Err(install_error("兼容 Skill 数量超过限制"));
        }
    }
    skills.sort();
    Ok(skills)
}

pub(super) fn compatible_root_skill_name(raw: &str) -> Result<String, String> {
    let normalized = raw.replace("\r\n", "\n");
    let frontmatter = normalized
        .strip_prefix("---\n")
        .and_then(|rest| {
            rest.split_once("\n---\n")
                .map(|(frontmatter, _)| frontmatter)
        })
        .ok_or_else(|| install_error("SKILL.md 缺少有效 YAML frontmatter"))?;
    let value = frontmatter
        .lines()
        .find_map(|line| line.trim().strip_prefix("name:").map(str::trim))
        .ok_or_else(|| install_error("SKILL.md 必须声明 name"))?;
    let value = if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    };
    if !valid_compatible_skill_name(value) {
        return Err(install_error("SKILL.md name 无效"));
    }
    Ok(value.to_owned())
}

pub(super) fn valid_compatible_skill_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
}

pub(super) fn copy_compatible_skill_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let mut visited = 0usize;
    copy_compatible_skill_tree_bounded(source, destination, &mut visited)
}

pub(super) fn copy_compatible_skill_tree_bounded(
    source: &Path,
    destination: &Path,
    visited: &mut usize,
) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(source).map_err(|_| install_error("无法读取兼容 Skill 资源"))?;
    if !metadata.is_dir() || is_symlink_or_reparse(&metadata) {
        return Err(install_error("兼容 Skill 资源目录无效"));
    }
    ensure_owned_directory(destination)
        .map_err(|_| install_error("无法创建兼容 Skill 资源目录"))?;
    for entry in fs::read_dir(source).map_err(|_| install_error("无法读取兼容 Skill 资源"))?
    {
        *visited = visited.saturating_add(1);
        if *visited > MAXIMUM_ARCHIVE_ENTRIES {
            return Err(install_error("兼容 Skill 资源条目超过限制"));
        }
        let entry = entry.map_err(|_| install_error("无法读取兼容 Skill 资源"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|_| install_error("无法读取兼容 Skill 资源"))?;
        if is_symlink_or_reparse(&metadata) {
            return Err(install_error("兼容 Skill 资源不允许链接或重解析点"));
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_compatible_skill_tree_bounded(&entry.path(), &target, visited)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), target).map_err(|_| install_error("无法复制兼容 Skill 资源"))?;
        } else {
            return Err(install_error("兼容 Skill 包含特殊文件"));
        }
    }
    Ok(())
}

pub(super) fn compatible_command(
    server: &serde_json::Map<String, Value>,
    server_name: &str,
) -> Result<Vec<String>, String> {
    let command = server
        .get("command")
        .ok_or_else(|| install_error(&format!("MCP {server_name} 缺少 command")))?;
    if let Some(command) = command.as_str() {
        return compatible_string_array(Some(&json!([command])), "MCP command");
    }
    let command = compatible_string_array(Some(command), "MCP command")?;
    if command.is_empty() {
        return Err(install_error("MCP command 不能为空"));
    }
    Ok(command)
}

pub(super) fn compatible_string_array(
    value: Option<&Value>,
    field: &str,
) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .filter(|values| values.len() <= 64)
        .ok_or_else(|| install_error(&format!("{field} 必须是字符串数组")))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.contains('\0') && value.len() <= 2_048)
                .map(str::to_owned)
                .ok_or_else(|| install_error(&format!("{field} 包含无效值")))
        })
        .collect()
}

pub(super) fn compatibility_identifier(value: &str, field: &str) -> Result<String, String> {
    let mut normalized = String::new();
    for character in value.trim().chars() {
        let character = character.to_ascii_lowercase();
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            normalized.push(character);
        } else if !normalized.ends_with('-') {
            normalized.push('-');
        }
    }
    let normalized = normalized
        .trim_matches(|character| matches!(character, '-' | '.' | '_'))
        .chars()
        .take(128)
        .collect::<String>();
    if normalized.is_empty() || !valid_executable_name(&normalized) {
        return Err(install_error(&format!("{field} 无法转换为安全标识")));
    }
    Ok(normalized)
}

pub(super) fn prepare_compatible_launcher(
    package_root: &Path,
    bin_root: &Path,
    component_key: &str,
    raw_command: &str,
    frozen_node_prepared: &mut bool,
) -> Result<String, String> {
    let command = raw_command.trim();
    if command.eq_ignore_ascii_case("node") || command.eq_ignore_ascii_case("node.exe") {
        let bin = "compat-node";
        let launcher = bin_root.join(format!("{bin}.exe"));
        if !*frozen_node_prepared {
            if launcher.exists() {
                return Err(install_error("兼容 MCP Node.js 启动器名称冲突"));
            }
            let node = find_system_executable("node")
                .ok_or_else(|| install_error("兼容 MCP 的 Node command 需要本机 Node.js"))?;
            fs::copy(node, &launcher)
                .map_err(|_| install_error("无法冻结兼容 MCP Node.js 运行时"))?;
            *frozen_node_prepared = true;
        }
        return Ok(bin.into());
    }

    let mut relative = command.replace('\\', "/");
    for prefix in [
        "${PLUGIN_ROOT}/",
        "$PLUGIN_ROOT/",
        "${CLAUDE_PLUGIN_ROOT}/",
        "$CLAUDE_PLUGIN_ROOT/",
    ] {
        if let Some(stripped) = relative.strip_prefix(prefix) {
            relative = stripped.to_owned();
            break;
        }
    }
    while let Some(stripped) = relative.strip_prefix("./") {
        relative = stripped.to_owned();
    }
    let relative = safe_relative_path(Path::new(&relative)).map_err(|_| {
        install_error("兼容 MCP command 必须是 node 或 Plugin 目录内的 Windows .exe")
    })?;
    if !relative
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
    {
        return Err(install_error(
            "兼容 MCP command 必须是 node 或 Plugin 目录内的 Windows .exe",
        ));
    }
    let source = package_root.join(relative);
    let metadata = fs::symlink_metadata(&source)
        .map_err(|_| install_error("兼容 MCP command 指向的可执行文件不存在"))?;
    if !metadata.is_file() || is_symlink_or_reparse(&metadata) {
        return Err(install_error("兼容 MCP command 指向的文件无效"));
    }
    let source = source
        .canonicalize()
        .map_err(|_| install_error("兼容 MCP command 路径无效"))?;
    let canonical_root = package_root
        .canonicalize()
        .map_err(|_| install_error("兼容 MCP 目录无效"))?;
    if !source.starts_with(&canonical_root) {
        return Err(install_error("兼容 MCP command 越界"));
    }
    let bin = format!("compat-{component_key}");
    let launcher = bin_root.join(format!("{bin}.exe"));
    if launcher.exists() {
        return Err(install_error("兼容 MCP 启动器名称冲突"));
    }
    fs::copy(source, launcher).map_err(|_| install_error("无法冻结兼容 MCP 可执行文件"))?;
    Ok(bin)
}
