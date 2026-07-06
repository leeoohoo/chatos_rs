// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::models::project_run::ProjectRunTarget;
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;

use super::scan_budget::{
    read_to_string_limited, ScanBudget, MAX_MANIFEST_BYTES, MAX_SOURCE_PROBE_BYTES,
};
use super::target_model::{build_target, push_target};

#[derive(Debug, Clone, Default)]
struct MavenPomInfo {
    packaging: Option<String>,
    modules: Vec<String>,
    spring_boot_main_classes: Vec<String>,
    has_spring_boot_plugin: bool,
    has_spring_boot_dependency: bool,
}

fn xml_local_name(raw: &[u8]) -> String {
    std::str::from_utf8(raw)
        .ok()
        .and_then(|value| value.rsplit(':').next())
        .unwrap_or_default()
        .to_string()
}

fn stack_ends_with(stack: &[String], suffix: &[&str]) -> bool {
    if stack.len() < suffix.len() {
        return false;
    }
    let offset = stack.len() - suffix.len();
    suffix
        .iter()
        .enumerate()
        .all(|(index, expected)| stack[offset + index] == *expected)
}

fn normalize_maven_module_path(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn parse_maven_pom(pom_path: &Path) -> MavenPomInfo {
    let Some(content) = read_to_string_limited(pom_path, MAX_MANIFEST_BYTES) else {
        return MavenPomInfo::default();
    };

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut info = MavenPomInfo::default();
    let mut seen_main_classes = HashSet::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                stack.push(xml_local_name(event.name().as_ref()));
            }
            Ok(Event::End(_)) => {
                stack.pop();
            }
            Ok(Event::Text(event)) => {
                let value = event
                    .decode()
                    .ok()
                    .and_then(|value| {
                        quick_xml::escape::unescape(value.as_ref())
                            .ok()
                            .map(|value| value.trim().to_string())
                    })
                    .unwrap_or_default();
                if value.is_empty() {
                    buf.clear();
                    continue;
                }
                if stack == ["project", "packaging"] {
                    info.packaging = Some(value);
                } else if stack_ends_with(&stack, &["modules", "module"]) {
                    let module = normalize_maven_module_path(&value);
                    if !module.is_empty() {
                        info.modules.push(module);
                    }
                } else if stack_ends_with(&stack, &["plugin", "artifactId"])
                    && value == "spring-boot-maven-plugin"
                {
                    info.has_spring_boot_plugin = true;
                } else if stack_ends_with(&stack, &["dependency", "artifactId"])
                    && value.starts_with("spring-boot")
                {
                    info.has_spring_boot_dependency = true;
                } else if stack_ends_with(&stack, &["configuration", "mainClass"])
                    && seen_main_classes.insert(value.clone())
                {
                    info.spring_boot_main_classes.push(value);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    info
}

fn path_relative_to(base: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    let normalized = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    let normalized = normalize_maven_module_path(&normalized);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn find_maven_reactor_invocation(
    scan_root: &Path,
    module_dir: &Path,
    budget: &mut ScanBudget,
) -> Result<Option<(PathBuf, String)>, String> {
    for ancestor in module_dir.ancestors().skip(1) {
        budget.check()?;
        if !ancestor.starts_with(scan_root) {
            continue;
        }
        let pom_path = ancestor.join("pom.xml");
        if !pom_path.is_file() {
            continue;
        }
        let Some(module_path) = path_relative_to(ancestor, module_dir) else {
            continue;
        };
        let pom_info = parse_maven_pom(&pom_path);
        if pom_info.modules.iter().any(|module| module == &module_path) {
            return Ok(Some((ancestor.to_path_buf(), module_path)));
        }
    }
    Ok(None)
}

fn merge_java_entrypoints(mut detected: Vec<String>, configured: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = detected.iter().cloned().collect();
    for entrypoint in configured {
        if seen.insert(entrypoint.clone()) {
            detected.push(entrypoint.clone());
        }
    }
    detected.sort();
    detected
}

fn format_maven_label(module_path: Option<&str>, suffix: &str) -> String {
    match module_path {
        Some(module) if !module.trim().is_empty() => {
            format!("Java(Maven): {module} {suffix}")
        }
        _ => format!("Java(Maven): {suffix}"),
    }
}

fn maven_command_prefix(cwd: &Path, module_path: Option<&str>) -> (String, Vec<&'static str>) {
    let has_mvnw = cwd.join("mvnw").is_file();
    let runner = if has_mvnw { "./mvnw" } else { "mvn" };
    let required_toolchains = if has_mvnw {
        vec!["java_home"]
    } else {
        vec!["java_home", "mvn"]
    };
    let prefix = match module_path {
        Some(module) if !module.trim().is_empty() => {
            format!("{runner} -pl {module} -am")
        }
        _ => runner.to_string(),
    };
    (prefix, required_toolchains)
}

pub(super) fn detect_java_targets(
    scan_root: &Path,
    dir: &Path,
    out: &mut Vec<ProjectRunTarget>,
    budget: &mut ScanBudget,
) -> Result<(), String> {
    let has_pom = dir.join("pom.xml").is_file();
    let has_gradle = dir.join("build.gradle").is_file()
        || dir.join("build.gradle.kts").is_file()
        || dir.join("settings.gradle").is_file()
        || dir.join("settings.gradle.kts").is_file();
    if !has_pom && !has_gradle {
        return Ok(());
    }
    let has_gradlew = dir.join("gradlew").is_file();
    let detected_java_entrypoints = detect_java_entrypoints(dir, budget)?;
    let pom_path = dir.join("pom.xml");
    let gradle_manifest = if dir.join("build.gradle").is_file() {
        Some(dir.join("build.gradle"))
    } else if dir.join("build.gradle.kts").is_file() {
        Some(dir.join("build.gradle.kts"))
    } else if dir.join("settings.gradle").is_file() {
        Some(dir.join("settings.gradle"))
    } else if dir.join("settings.gradle.kts").is_file() {
        Some(dir.join("settings.gradle.kts"))
    } else {
        None
    };

    if has_pom {
        let pom_info = parse_maven_pom(&pom_path);
        let packaging = pom_info.packaging.as_deref().unwrap_or("jar");
        let reactor_invocation = find_maven_reactor_invocation(scan_root, dir, budget)?;
        let invocation_cwd_path = reactor_invocation
            .as_ref()
            .map(|(root, _)| root.as_path())
            .unwrap_or(dir);
        let module_path = reactor_invocation
            .as_ref()
            .map(|(_, module)| module.as_str());
        let cwd = invocation_cwd_path.to_string_lossy().to_string();
        let manifest_path = Some(pom_path.to_string_lossy().to_string());
        let (command_prefix, required_toolchains) =
            maven_command_prefix(invocation_cwd_path, module_path);
        let java_entrypoints = merge_java_entrypoints(
            detected_java_entrypoints.clone(),
            &pom_info.spring_boot_main_classes,
        );
        let has_spring_boot_hint = pom_info.has_spring_boot_plugin
            || pom_info.has_spring_boot_dependency
            || !pom_info.spring_boot_main_classes.is_empty();
        if java_entrypoints.is_empty() {
            if has_spring_boot_hint && packaging != "pom" {
                push_target(
                    out,
                    build_target(
                        cwd.as_str(),
                        format_maven_label(module_path, "spring-boot:run"),
                        "java",
                        format!("{command_prefix} spring-boot:run"),
                        0.9,
                        None,
                        manifest_path.clone(),
                        required_toolchains.clone(),
                    ),
                );
            }
        } else {
            for entrypoint in &java_entrypoints {
                let (label_suffix, command, confidence) = if has_spring_boot_hint {
                    (
                        entrypoint.as_str(),
                        if pom_info.spring_boot_main_classes.len() == 1
                            && pom_info.spring_boot_main_classes[0] == *entrypoint
                        {
                            format!("{command_prefix} spring-boot:run")
                        } else {
                            format!(
                                "{command_prefix} -Dspring-boot.run.main-class={} spring-boot:run",
                                entrypoint
                            )
                        },
                        0.96,
                    )
                } else {
                    (
                        entrypoint.as_str(),
                        format!("{command_prefix} -Dexec.mainClass={} exec:java", entrypoint),
                        0.94,
                    )
                };
                push_target(
                    out,
                    build_target(
                        cwd.as_str(),
                        format_maven_label(module_path, label_suffix),
                        "java",
                        command,
                        confidence,
                        Some(entrypoint.clone()),
                        manifest_path.clone(),
                        required_toolchains.clone(),
                    ),
                );
            }
        }
        push_target(
            out,
            build_target(
                cwd.as_str(),
                format_maven_label(module_path, "test"),
                "java",
                format!("{command_prefix} test"),
                0.8,
                None,
                manifest_path,
                required_toolchains,
            ),
        );
    }
    if has_gradle {
        let cwd = dir.to_string_lossy().to_string();
        let java_entrypoints = detected_java_entrypoints;
        let runner = if has_gradlew { "./gradlew" } else { "gradle" };
        let required_toolchains = if has_gradlew {
            vec!["java_home", "gradle_user_home"]
        } else {
            vec!["java_home", "gradle", "gradle_user_home"]
        };
        if java_entrypoints.is_empty() {
            push_target(
                out,
                build_target(
                    cwd.as_str(),
                    "Java(Gradle): bootRun".to_string(),
                    "java",
                    format!("{runner} bootRun"),
                    0.88,
                    None,
                    gradle_manifest
                        .as_ref()
                        .map(|path| path.to_string_lossy().to_string()),
                    required_toolchains.clone(),
                ),
            );
        } else {
            for entrypoint in &java_entrypoints {
                push_target(
                    out,
                    build_target(
                        cwd.as_str(),
                        format!("Java(Gradle): {}", entrypoint),
                        "java",
                        format!("{runner} run -PmainClass={entrypoint}"),
                        0.9,
                        Some(entrypoint.clone()),
                        gradle_manifest
                            .as_ref()
                            .map(|path| path.to_string_lossy().to_string()),
                        required_toolchains.clone(),
                    ),
                );
            }
        }
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Java(Gradle): test".to_string(),
                "java",
                format!("{runner} test"),
                0.78,
                None,
                gradle_manifest
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                required_toolchains,
            ),
        );
    }
    Ok(())
}

fn detect_java_entrypoints(dir: &Path, budget: &mut ScanBudget) -> Result<Vec<String>, String> {
    let src_root = dir.join("src").join("main").join("java");
    if !src_root.is_dir() {
        return Ok(Vec::new());
    }

    let package_re = Regex::new(r"(?m)^\s*package\s+([A-Za-z_][A-Za-z0-9_.]*)\s*;").ok();
    let main_re = Regex::new(r"public\s+static\s+void\s+main\s*\(\s*String(?:\[\]|\s*\.\.\.)").ok();
    let class_re = Regex::new(r"\bclass\s+([A-Za-z_][A-Za-z0-9_]*)").ok();

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for entry in walkdir::WalkDir::new(&src_root).into_iter().flatten() {
        budget.account_entry()?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("java") {
            continue;
        }
        let Some(content) = read_to_string_limited(entry.path(), MAX_SOURCE_PROBE_BYTES) else {
            continue;
        };
        if !main_re.as_ref().is_some_and(|re| re.is_match(&content)) {
            continue;
        }
        let class_name = class_re
            .as_ref()
            .and_then(|re| re.captures(&content))
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_string());
        let Some(class_name) = class_name else {
            continue;
        };
        let package_name = package_re
            .as_ref()
            .and_then(|re| re.captures(&content))
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_string());
        let fqcn = match package_name {
            Some(package) if !package.trim().is_empty() => format!("{package}.{class_name}"),
            _ => class_name,
        };
        if seen.insert(fqcn.clone()) {
            out.push(fqcn);
        }
    }
    out.sort();
    Ok(out)
}
