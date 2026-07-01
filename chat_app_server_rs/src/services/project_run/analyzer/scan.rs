// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;

use crate::models::project_run::ProjectRunTarget;

use super::go::detect_go_targets;
use super::java::detect_java_targets;
use super::node::detect_node_targets;
use super::python::detect_python_targets;
use super::rust::detect_rust_targets;
use super::scan_budget::ScanBudget;
use super::target_model::MAX_TARGETS;

#[cfg(test)]
use super::target_model::is_same_cwd;

const MAX_SCAN_DIRS: usize = 2500;
const MAX_SCAN_DEPTH: usize = 6;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    ".next",
    ".venv",
    "venv",
    "target",
    ".idea",
    ".vscode",
];

fn default_target_priority(target: &ProjectRunTarget) -> i32 {
    let cmd = target.command.to_lowercase();
    if target.kind == "node"
        && (cmd.contains("npm run dev") || cmd == "pnpm dev" || cmd == "yarn dev")
    {
        return 100;
    }
    if target.kind == "node"
        && (cmd.contains("npm run start") || cmd == "pnpm start" || cmd == "yarn start")
    {
        return 95;
    }
    if target.kind == "java" && cmd.contains("spring-boot:run") {
        return 92;
    }
    if target.kind == "java" && cmd.contains("bootrun") {
        return 90;
    }
    if target.kind == "java" && cmd.contains("exec:java") {
        return 89;
    }
    if target.kind == "python" && cmd.contains("main.py") {
        return 88;
    }
    if target.kind == "go" && cmd.starts_with("go run ./cmd/") {
        return 87;
    }
    if target.kind == "go" && cmd == "go run ." {
        return 85;
    }
    if target.kind == "rust" && cmd == "cargo run" {
        return 84;
    }
    if target.kind == "rust" && cmd.starts_with("cargo run --bin ") {
        return 83;
    }
    if cmd.contains("test") {
        return 40;
    }
    70
}

pub(super) fn detect_targets_sync(root: PathBuf) -> Result<Vec<ProjectRunTarget>, String> {
    let mut budget = ScanBudget::for_project_run_analysis();
    detect_targets_with_budget(root, &mut budget)
}

fn detect_targets_with_budget(
    root: PathBuf,
    budget: &mut ScanBudget,
) -> Result<Vec<ProjectRunTarget>, String> {
    if !root.exists() || !root.is_dir() {
        return Err("项目目录不存在或不可访问".to_string());
    }

    let mut targets: Vec<ProjectRunTarget> = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.clone(), 0));
    let mut visited = 0usize;

    while let Some((dir, depth)) = queue.pop_front() {
        if visited >= MAX_SCAN_DIRS || targets.len() >= MAX_TARGETS {
            break;
        }
        budget.account_entry()?;
        visited += 1;

        let mut file_names: HashSet<String> = HashSet::new();
        let entries = match fs::read_dir(&dir) {
            Ok(iter) => iter,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            budget.account_entry()?;
            let name = entry.file_name().to_string_lossy().to_string();
            let lower_name = name.to_lowercase();
            let path = entry.path();
            if path.is_dir() {
                if depth < MAX_SCAN_DEPTH && !IGNORED_DIRS.contains(&lower_name.as_str()) {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }
            file_names.insert(lower_name);
        }

        detect_node_targets(&dir, &mut targets);
        detect_java_targets(&root, &dir, &mut targets, budget)?;
        detect_python_targets(&dir, &file_names, &mut targets);
        detect_go_targets(&dir, &file_names, &mut targets, budget)?;
        detect_rust_targets(&dir, &file_names, &mut targets, budget)?;
    }

    targets.sort_by(|a, b| {
        default_target_priority(b)
            .cmp(&default_target_priority(a))
            .then_with(|| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.label.cmp(&b.label))
    });

    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::{detect_targets_sync, detect_targets_with_budget, is_same_cwd};
    use crate::services::project_run::analyzer::scan_budget::ScanBudget;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "chatos_project_run_{name}_{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create test directory");
        }
        fs::write(path, content).expect("write test file");
    }

    #[test]
    fn detects_maven_reactor_spring_boot_modules() {
        let root = temp_root("maven_reactor");
        let _ = fs::remove_dir_all(&root);

        write_file(
            &root.join("pom.xml"),
            r#"
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>demo-parent</artifactId>
  <version>1.0.0</version>
  <packaging>pom</packaging>
  <modules>
    <module>admin-server</module>
    <module>agent-server</module>
    <module>common-lib</module>
  </modules>
</project>
"#,
        );

        write_file(
            &root.join("admin-server/pom.xml"),
            r#"
<project>
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>demo-parent</artifactId>
    <version>1.0.0</version>
  </parent>
  <artifactId>admin-server</artifactId>
  <build>
    <plugins>
      <plugin>
        <groupId>org.springframework.boot</groupId>
        <artifactId>spring-boot-maven-plugin</artifactId>
        <configuration>
          <mainClass>com.example.admin.AdminApplication</mainClass>
        </configuration>
      </plugin>
    </plugins>
  </build>
</project>
"#,
        );
        write_file(
            &root.join("admin-server/src/main/java/com/example/admin/AdminApplication.java"),
            r#"
package com.example.admin;

public class AdminApplication {
    public static void main(String[] args) {}
}
"#,
        );

        write_file(
            &root.join("agent-server/pom.xml"),
            r#"
<project>
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>demo-parent</artifactId>
    <version>1.0.0</version>
  </parent>
  <artifactId>agent-server</artifactId>
  <build>
    <plugins>
      <plugin>
        <groupId>org.springframework.boot</groupId>
        <artifactId>spring-boot-maven-plugin</artifactId>
        <configuration>
          <mainClass>com.example.agent.AgentApplication</mainClass>
        </configuration>
      </plugin>
    </plugins>
  </build>
</project>
"#,
        );
        write_file(
            &root.join("agent-server/src/main/java/com/example/agent/AgentApplication.java"),
            r#"
package com.example.agent;

public class AgentApplication {
    public static void main(String[] args) {}
}
"#,
        );

        write_file(
            &root.join("common-lib/pom.xml"),
            r#"
<project>
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>demo-parent</artifactId>
    <version>1.0.0</version>
  </parent>
  <artifactId>common-lib</artifactId>
</project>
"#,
        );

        let targets = detect_targets_sync(root.clone()).expect("detect targets");
        let commands = targets
            .iter()
            .map(|target| target.command.as_str())
            .collect::<Vec<_>>();

        assert!(commands.contains(&"mvn -pl admin-server -am spring-boot:run"));
        assert!(commands.contains(&"mvn -pl agent-server -am spring-boot:run"));
        assert!(!targets.iter().any(|target| {
            is_same_cwd(target.cwd.as_str(), root.to_string_lossy().as_ref())
                && target.command == "mvn spring-boot:run"
        }));
        assert!(!commands.iter().any(
            |command| command.contains("-pl common-lib") && command.contains("spring-boot:run")
        ));
        let expected_admin_pom = root.join("admin-server").join("pom.xml");
        assert!(targets.iter().any(|target| {
            target.entrypoint.as_deref() == Some("com.example.admin.AdminApplication")
                && target.manifest_path.as_deref().map(PathBuf::from).as_ref()
                    == Some(&expected_admin_pom)
        }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stops_when_project_scan_budget_is_exhausted() {
        let root = temp_root("scan_budget");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create root");
        for index in 0..8 {
            fs::create_dir_all(root.join(format!("module_{index}"))).expect("create module");
        }

        let mut budget = ScanBudget::for_test(2, Duration::from_secs(60));
        let err = detect_targets_with_budget(root.clone(), &mut budget)
            .expect_err("scan should stop when budget is exhausted");

        assert!(err.contains("filesystem entries"));
        let _ = fs::remove_dir_all(&root);
    }
}
