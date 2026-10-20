use super::models::{
    AnalyzedProjectRunTarget, DiscoveredToolchain, ProjectRunAnalysis, ProjectRunConfigurationFile,
    ProjectRunExecutable, ProjectRunInvocation, ProjectRunTarget, ProjectRunToolchainOption,
    ResolvedProjectRunTarget,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAXIMUM_DIRECTORIES: usize = 2_500;
const MAXIMUM_DEPTH: usize = 6;
const MAXIMUM_CHILDREN_PER_DIRECTORY: usize = 10_000;
const MAXIMUM_TARGETS: usize = 100;
const MAXIMUM_CONFIGURATION_FILES: usize = 60;
const MAXIMUM_MANIFEST_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_CONFIGURATION_PREVIEW_BYTES: u64 = 128 * 1_024;
const MAXIMUM_CONFIGURATION_PREVIEW_CHARACTERS: usize = 12_000;

const IGNORED_DIRECTORIES: &[&str] = &[
    ".chatos",
    ".git",
    ".idea",
    ".next",
    ".venv",
    ".vscode",
    "build",
    "dist",
    "node_modules",
    "target",
    "venv",
    "vendor",
];

const DETECTION_FILE_NAMES: &[&str] = &[
    "app.py",
    "build.gradle",
    "build.gradle.kts",
    "Cargo.toml",
    "go.mod",
    "gradlew",
    "gradlew.bat",
    "main.py",
    "mvnw",
    "mvnw.cmd",
    "package.json",
    "Package.swift",
    "pnpm-lock.yaml",
    "pom.xml",
    "pyproject.toml",
    "pytest.ini",
    "requirements.txt",
    "yarn.lock",
];

pub(crate) fn analyze_project(root: &Path) -> Result<ProjectRunAnalysis, String> {
    let canonical_root = fs::canonicalize(root).map_err(|_| unavailable())?;
    let metadata = fs::symlink_metadata(&canonical_root).map_err(|_| unavailable())?;
    if !metadata.is_dir() || is_link_or_reparse_point(&metadata) {
        return Err(unavailable());
    }

    let mut queue = VecDeque::from([(canonical_root.clone(), 0_usize)]);
    let mut visited = 0_usize;
    let mut targets = Vec::new();
    let mut configurations = HashSet::new();

    while let Some((directory, depth)) = queue.pop_front() {
        if visited >= MAXIMUM_DIRECTORIES || targets.len() >= MAXIMUM_TARGETS {
            break;
        }
        visited += 1;
        let mut files = known_detection_files(&directory);
        let children = match fs::read_dir(&directory) {
            Ok(children) => children,
            Err(_) => continue,
        };
        for child in children.take(MAXIMUM_CHILDREN_PER_DIRECTORY).flatten() {
            let path = child.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if is_link_or_reparse_point(&metadata) {
                continue;
            }
            let Some(name) = child.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.chars().any(char::is_control) {
                continue;
            }
            let lowercase = name.to_lowercase();
            if metadata.is_dir() {
                if depth < MAXIMUM_DEPTH && !IGNORED_DIRECTORIES.contains(&lowercase.as_str()) {
                    queue.push_back((path, depth + 1));
                }
            } else if metadata.is_file() {
                files.insert(lowercase, path);
            }
        }

        detect_node(
            &canonical_root,
            &directory,
            &files,
            &mut targets,
            &mut configurations,
        );
        detect_rust(
            &canonical_root,
            &directory,
            &files,
            &mut targets,
            &mut configurations,
        );
        detect_python(
            &canonical_root,
            &directory,
            &files,
            &mut targets,
            &mut configurations,
        );
        detect_go(
            &canonical_root,
            &directory,
            &files,
            &mut targets,
            &mut configurations,
        );
        detect_java(
            &canonical_root,
            &directory,
            &files,
            &mut targets,
            &mut configurations,
        );
        detect_swift(
            &canonical_root,
            &directory,
            &files,
            &mut targets,
            &mut configurations,
        );
    }

    let mut seen = HashSet::new();
    targets.retain(|target| seen.insert(target.presentation.id.clone()));
    targets.sort_by(|left, right| {
        priority(right)
            .cmp(&priority(left))
            .then_with(|| left.presentation.label.cmp(&right.presentation.label))
    });
    if let Some(default_id) = targets.first().map(|target| target.presentation.id.clone()) {
        for target in &mut targets {
            target.presentation.is_default = target.presentation.id == default_id;
        }
    }

    let mut configuration_paths = configurations.into_iter().collect::<Vec<_>>();
    configuration_paths.sort();
    let configuration_files = configuration_paths
        .into_iter()
        .take(MAXIMUM_CONFIGURATION_FILES)
        .filter_map(|path| configuration_file(&canonical_root, &path))
        .collect();

    Ok(ProjectRunAnalysis {
        targets,
        configuration_files,
        toolchains: discover_toolchains(),
    })
}

pub(crate) fn resolve_project_run_target(
    root: &Path,
    target_id: &str,
) -> Result<ResolvedProjectRunTarget, String> {
    let canonical_root = fs::canonicalize(root).map_err(|_| unavailable())?;
    let analysis = analyze_project(&canonical_root)?;
    let target = analysis
        .targets
        .into_iter()
        .find(|target| target.presentation.id == target_id)
        .ok_or_else(|| "运行目标不存在，请重新分析项目。".to_owned())?;
    let cwd = resolve_project_path(&canonical_root, &target.presentation.cwd, true)?;
    let executable = match &target.invocation.executable {
        ProjectRunExecutable::Toolchain(kind) => analysis
            .toolchains
            .get(kind)
            .and_then(|options| options.first())
            .map(|option| option.executable.clone())
            .ok_or_else(|| format!("没有发现可用的 {kind} 工具链。"))?,
        ProjectRunExecutable::ProjectFile(path) => {
            resolve_project_path(&canonical_root, path, false)?
        }
    };
    Ok(ResolvedProjectRunTarget {
        presentation: target.presentation,
        executable,
        arguments: target.invocation.arguments,
        cwd,
    })
}

fn resolve_project_path(root: &Path, relative: &str, directory: bool) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.starts_with('\\')
        || relative.chars().any(char::is_control)
    {
        return Err(unavailable());
    }
    let mut path = root.to_path_buf();
    if relative != "." {
        for component in relative.split('/') {
            if component.is_empty() || component == "." || component == ".." {
                return Err(unavailable());
            }
            path.push(component);
            let metadata = fs::symlink_metadata(&path).map_err(|_| unavailable())?;
            if is_link_or_reparse_point(&metadata) {
                return Err(unavailable());
            }
        }
    }
    let canonical = fs::canonicalize(&path).map_err(|_| unavailable())?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|_| unavailable())?;
    if !canonical.starts_with(root)
        || is_link_or_reparse_point(&metadata)
        || directory != metadata.is_dir()
    {
        return Err(unavailable());
    }
    Ok(canonical)
}

fn known_detection_files(directory: &Path) -> HashMap<String, PathBuf> {
    let mut files = HashMap::new();
    for name in DETECTION_FILE_NAMES {
        let path = directory.join(name);
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_file() && !is_link_or_reparse_point(&metadata) {
            files.insert(name.to_lowercase(), path);
        }
    }
    files
}

fn detect_node(
    root: &Path,
    directory: &Path,
    files: &HashMap<String, PathBuf>,
    output: &mut Vec<AnalyzedProjectRunTarget>,
    configs: &mut HashSet<PathBuf>,
) {
    let Some(manifest) = files.get("package.json") else {
        return;
    };
    configs.insert(manifest.clone());
    let Some(bytes) = bounded_file(manifest, MAXIMUM_MANIFEST_BYTES) else {
        return;
    };
    let Ok(object) = serde_json::from_slice::<Value>(&bytes) else {
        return;
    };
    let manager = if files.contains_key("pnpm-lock.yaml") {
        "pnpm"
    } else if files.contains_key("yarn.lock") {
        "yarn"
    } else {
        "npm"
    };
    let scripts = object.get("scripts").and_then(Value::as_object);
    for name in ["dev", "start", "serve", "test", "build"] {
        if scripts.is_none_or(|scripts| !scripts.contains_key(name)) {
            continue;
        }
        let arguments = if manager == "npm" {
            vec!["run".into(), name.into()]
        } else {
            vec![name.into()]
        };
        let command = if manager == "npm" {
            format!("npm run {name}")
        } else {
            format!("{manager} {name}")
        };
        output.push(target(
            root,
            directory,
            format!("Node: {manager} {name}"),
            "node",
            Some("JavaScript / TypeScript"),
            command,
            format!("package.json:scripts.{name}"),
            None,
            Some(manifest),
            vec!["node", manager],
            ProjectRunInvocation {
                executable: ProjectRunExecutable::Toolchain(manager.into()),
                arguments,
            },
        ));
    }
}

fn detect_rust(
    root: &Path,
    directory: &Path,
    files: &HashMap<String, PathBuf>,
    output: &mut Vec<AnalyzedProjectRunTarget>,
    configs: &mut HashSet<PathBuf>,
) {
    let Some(manifest) = files.get("cargo.toml") else {
        return;
    };
    configs.insert(manifest.clone());
    if safe_regular_file(&directory.join("src").join("main.rs")) {
        output.push(toolchain_target(
            root,
            directory,
            "Rust: cargo run",
            "rust",
            "Rust",
            "cargo run",
            "Cargo.toml",
            Some("src/main.rs"),
            manifest,
            "cargo",
            vec!["run"],
        ));
    }
    output.push(toolchain_target(
        root,
        directory,
        "Rust: cargo test",
        "rust",
        "Rust",
        "cargo test",
        "Cargo.toml",
        None,
        manifest,
        "cargo",
        vec!["test"],
    ));
}

fn detect_python(
    root: &Path,
    directory: &Path,
    files: &HashMap<String, PathBuf>,
    output: &mut Vec<AnalyzedProjectRunTarget>,
    configs: &mut HashSet<PathBuf>,
) {
    let manifest = files
        .get("pyproject.toml")
        .or_else(|| files.get("requirements.txt"));
    if let Some(manifest) = manifest {
        configs.insert(manifest.clone());
    }
    for name in ["main.py", "app.py"] {
        if let Some(entrypoint) = files.get(name) {
            let source = name.to_owned();
            output.push(target(
                root,
                directory,
                format!("Python: {name}"),
                "python",
                Some("Python"),
                format!("python {name}"),
                source,
                Some(name),
                manifest
                    .map(PathBuf::as_path)
                    .or(Some(entrypoint.as_path())),
                vec!["python"],
                ProjectRunInvocation {
                    executable: ProjectRunExecutable::Toolchain("python".into()),
                    arguments: vec![name.into()],
                },
            ));
        }
    }
    if manifest.is_some() || files.contains_key("pytest.ini") {
        output.push(target(
            root,
            directory,
            "Python: pytest".into(),
            "python",
            Some("Python"),
            "python -m pytest".into(),
            manifest
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("pytest.ini")
                .into(),
            None,
            manifest.map(PathBuf::as_path),
            vec!["python"],
            ProjectRunInvocation {
                executable: ProjectRunExecutable::Toolchain("python".into()),
                arguments: vec!["-m".into(), "pytest".into()],
            },
        ));
    }
}

fn detect_go(
    root: &Path,
    directory: &Path,
    files: &HashMap<String, PathBuf>,
    output: &mut Vec<AnalyzedProjectRunTarget>,
    configs: &mut HashSet<PathBuf>,
) {
    let Some(manifest) = files.get("go.mod") else {
        return;
    };
    configs.insert(manifest.clone());
    output.push(toolchain_target(
        root,
        directory,
        "Go: run",
        "go",
        "Go",
        "go run .",
        "go.mod",
        Some("."),
        manifest,
        "go",
        vec!["run", "."],
    ));
    output.push(toolchain_target(
        root,
        directory,
        "Go: test",
        "go",
        "Go",
        "go test ./...",
        "go.mod",
        None,
        manifest,
        "go",
        vec!["test", "./..."],
    ));
}

fn detect_java(
    root: &Path,
    directory: &Path,
    files: &HashMap<String, PathBuf>,
    output: &mut Vec<AnalyzedProjectRunTarget>,
    configs: &mut HashSet<PathBuf>,
) {
    if let Some(manifest) = files.get("pom.xml") {
        configs.insert(manifest.clone());
        let wrapper = files.get("mvnw.cmd").or_else(|| files.get("mvnw"));
        let invocation = wrapper.map_or_else(
            || ProjectRunInvocation {
                executable: ProjectRunExecutable::Toolchain("mvn".into()),
                arguments: vec!["spring-boot:run".into()],
            },
            |wrapper| ProjectRunInvocation {
                executable: ProjectRunExecutable::ProjectFile(
                    relative_path(root, wrapper).unwrap_or_default(),
                ),
                arguments: vec!["spring-boot:run".into()],
            },
        );
        let required = if wrapper.is_some() {
            vec!["java"]
        } else {
            vec!["java", "mvn"]
        };
        output.push(target(
            root,
            directory,
            "Java: Spring Boot (Maven)".into(),
            "java",
            Some("Java"),
            if wrapper.is_some() {
                "mvnw spring-boot:run".into()
            } else {
                "mvn spring-boot:run".into()
            },
            "pom.xml".into(),
            None,
            Some(manifest),
            required,
            invocation,
        ));
    }
    if let Some(manifest) = files
        .get("build.gradle")
        .or_else(|| files.get("build.gradle.kts"))
    {
        configs.insert(manifest.clone());
        let wrapper = files.get("gradlew.bat").or_else(|| files.get("gradlew"));
        let invocation = wrapper.map_or_else(
            || ProjectRunInvocation {
                executable: ProjectRunExecutable::Toolchain("gradle".into()),
                arguments: vec!["bootRun".into()],
            },
            |wrapper| ProjectRunInvocation {
                executable: ProjectRunExecutable::ProjectFile(
                    relative_path(root, wrapper).unwrap_or_default(),
                ),
                arguments: vec!["bootRun".into()],
            },
        );
        let required = if wrapper.is_some() {
            vec!["java"]
        } else {
            vec!["java", "gradle"]
        };
        output.push(target(
            root,
            directory,
            "Java: Spring Boot (Gradle)".into(),
            "java",
            Some("Java / Kotlin"),
            if wrapper.is_some() {
                "gradlew bootRun".into()
            } else {
                "gradle bootRun".into()
            },
            manifest
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("build.gradle")
                .into(),
            None,
            Some(manifest),
            required,
            invocation,
        ));
    }
}

fn detect_swift(
    root: &Path,
    directory: &Path,
    files: &HashMap<String, PathBuf>,
    output: &mut Vec<AnalyzedProjectRunTarget>,
    configs: &mut HashSet<PathBuf>,
) {
    let Some(manifest) = files.get("package.swift") else {
        return;
    };
    configs.insert(manifest.clone());
    for action in ["run", "test"] {
        output.push(toolchain_target(
            root,
            directory,
            &format!("Swift: swift {action}"),
            "swift",
            "Swift",
            &format!("swift {action}"),
            "Package.swift",
            None,
            manifest,
            "swift",
            vec![action],
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn toolchain_target(
    root: &Path,
    directory: &Path,
    label: &str,
    kind: &str,
    language: &str,
    command: &str,
    source: &str,
    entrypoint: Option<&str>,
    manifest: &Path,
    toolchain: &str,
    arguments: Vec<&str>,
) -> AnalyzedProjectRunTarget {
    target(
        root,
        directory,
        label.into(),
        kind,
        Some(language),
        command.into(),
        source.into(),
        entrypoint,
        Some(manifest),
        vec![toolchain],
        ProjectRunInvocation {
            executable: ProjectRunExecutable::Toolchain(toolchain.into()),
            arguments: arguments.into_iter().map(str::to_owned).collect(),
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn target(
    root: &Path,
    directory: &Path,
    label: String,
    kind: &str,
    language: Option<&str>,
    command: String,
    source: String,
    entrypoint: Option<&str>,
    manifest: Option<&Path>,
    required_toolchains: Vec<&str>,
    invocation: ProjectRunInvocation,
) -> AnalyzedProjectRunTarget {
    let cwd = relative_path(root, directory).unwrap_or_else(|| ".".into());
    let identity = format!("{kind}\0{cwd}\0{command}");
    let id = format!("{:x}", Sha256::digest(identity.as_bytes()));
    AnalyzedProjectRunTarget {
        presentation: ProjectRunTarget {
            id: id[..20].into(),
            label,
            kind: kind.into(),
            language: language.map(str::to_owned),
            cwd,
            command,
            source,
            is_default: false,
            entrypoint: entrypoint.map(str::to_owned),
            manifest_path: manifest.and_then(|path| relative_path(root, path)),
            required_toolchains: required_toolchains.into_iter().map(str::to_owned).collect(),
        },
        invocation,
    }
}

fn priority(target: &AnalyzedProjectRunTarget) -> i32 {
    let command = target.presentation.command.to_lowercase();
    if command.contains("npm run dev") || command == "pnpm dev" || command == "yarn dev" {
        100
    } else if command.contains("npm run start")
        || command == "pnpm start"
        || command == "yarn start"
    {
        95
    } else if command.contains("spring-boot:run") {
        92
    } else if command.to_lowercase().contains("bootrun") {
        90
    } else if command.contains("main.py") {
        88
    } else if command == "go run ." {
        85
    } else if command == "cargo run" || command == "swift run" {
        84
    } else if command.contains("test") {
        40
    } else {
        70
    }
}

fn configuration_file(root: &Path, path: &Path) -> Option<ProjectRunConfigurationFile> {
    let relative = relative_path(root, path)?;
    let label = path.file_name()?.to_str()?.to_owned();
    let kind = label.to_lowercase();
    let preview = bounded_file(path, MAXIMUM_CONFIGURATION_PREVIEW_BYTES).and_then(|bytes| {
        if bytes.iter().take(8_000).any(|byte| *byte == 0) {
            return None;
        }
        Some(
            String::from_utf8_lossy(&bytes)
                .chars()
                .take(MAXIMUM_CONFIGURATION_PREVIEW_CHARACTERS)
                .collect(),
        )
    });
    Some(ProjectRunConfigurationFile {
        id: format!("{kind}:{relative}"),
        kind,
        label,
        path: relative,
        preview,
        source: "本机项目",
    })
}

fn discover_toolchains() -> BTreeMap<String, Vec<DiscoveredToolchain>> {
    let candidates = [
        ("node", &["node"] as &[&str]),
        ("npm", &["npm"] as &[&str]),
        ("pnpm", &["pnpm"] as &[&str]),
        ("yarn", &["yarn"] as &[&str]),
        ("python", &["python3", "python"] as &[&str]),
        ("cargo", &["cargo"] as &[&str]),
        ("go", &["go"] as &[&str]),
        ("java", &["java"] as &[&str]),
        ("mvn", &["mvn"] as &[&str]),
        ("gradle", &["gradle"] as &[&str]),
        ("swift", &["swift"] as &[&str]),
    ];
    candidates
        .into_iter()
        .filter_map(|(kind, names)| {
            let executable = names.iter().find_map(|name| find_executable(name))?;
            let label = executable
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(kind)
                .to_owned();
            let id = opaque_executable_id(kind, &executable);
            Some((
                kind.to_owned(),
                vec![DiscoveredToolchain {
                    presentation: ProjectRunToolchainOption {
                        id,
                        kind: kind.to_owned(),
                        label,
                        version: None,
                        source: "本机",
                        is_default: true,
                    },
                    executable,
                }],
            ))
        })
        .collect()
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    let extensions: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for directory in env::split_paths(&path) {
        for extension in extensions {
            let candidate = directory.join(format!("{name}{extension}"));
            let executable = if candidate.is_absolute() {
                candidate
            } else {
                env::current_dir().ok()?.join(candidate)
            };
            let Ok(metadata) = fs::symlink_metadata(&executable) else {
                continue;
            };
            if !metadata.is_file() && !is_link_or_reparse_point(&metadata) {
                continue;
            }
            let Ok(canonical) = fs::canonicalize(&executable) else {
                continue;
            };
            let Ok(resolved_metadata) = fs::symlink_metadata(&canonical) else {
                continue;
            };
            if resolved_metadata.is_file() && !is_link_or_reparse_point(&resolved_metadata) {
                return Some(executable);
            }
        }
    }
    None
}

fn opaque_executable_id(kind: &str, path: &Path) -> String {
    format!(
        "tool_{}",
        &format!(
            "{:x}",
            Sha256::digest(format!("{kind}\0{}", path.to_string_lossy()).as_bytes())
        )[..24]
    )
}

fn bounded_file(path: &Path, maximum: u64) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || is_link_or_reparse_point(&metadata) || metadata.len() > maximum {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .ok()?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= maximum).then_some(bytes)
}

fn safe_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.is_file() && !is_link_or_reparse_point(&metadata))
        .unwrap_or(false)
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    if relative.as_os_str().is_empty() {
        return Some(".".into());
    }
    let value = relative.to_str()?.replace('\\', "/");
    if value.is_empty()
        || value.starts_with('/')
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        None
    } else {
        Some(value)
    }
}

#[cfg(windows)]
fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn unavailable() -> String {
    "项目目录不存在或当前不可访问。".into()
}
