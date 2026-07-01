// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::path::Path;

use crate::models::project_run::ProjectRunTarget;

use super::target_model::{build_target, push_target};

pub(super) fn detect_python_targets(
    dir: &Path,
    files: &HashSet<String>,
    out: &mut Vec<ProjectRunTarget>,
) {
    let has_py_hint = files.contains("pyproject.toml")
        || files.contains("requirements.txt")
        || files.contains("main.py")
        || files.contains("app.py")
        || files.iter().any(|name| name.ends_with(".py"));
    if !has_py_hint {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
    let manifest_path = if files.contains("pyproject.toml") {
        Some(dir.join("pyproject.toml").to_string_lossy().to_string())
    } else if files.contains("requirements.txt") {
        Some(dir.join("requirements.txt").to_string_lossy().to_string())
    } else {
        None
    };
    if files.contains("main.py") {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Python: main.py".to_string(),
                "python",
                "python main.py".to_string(),
                0.9,
                Some("main.py".to_string()),
                manifest_path.clone(),
                vec!["python"],
            ),
        );
    }
    if files.contains("app.py") {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Python: app.py".to_string(),
                "python",
                "python app.py".to_string(),
                0.88,
                Some("app.py".to_string()),
                manifest_path.clone(),
                vec!["python"],
            ),
        );
    }
    if files.contains("pytest.ini")
        || files.contains("pyproject.toml")
        || files.contains("requirements.txt")
    {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Python: pytest".to_string(),
                "python",
                "pytest".to_string(),
                0.75,
                None,
                manifest_path,
                vec!["python"],
            ),
        );
    }
}
