#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectRunPathChangeKind {
    Catalog,
    Environment,
}

impl ProjectRunPathChangeKind {
    pub(crate) fn realtime_reason(self) -> &'static str {
        match self {
            Self::Catalog => "project_run_catalog_changed",
            Self::Environment => "project_run_environment_changed",
        }
    }
}

fn normalize_project_run_change_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_lowercase()
}

fn is_create_or_delete_change(change_kind: Option<&str>) -> bool {
    matches!(
        change_kind.map(|value| value.trim().to_ascii_lowercase()),
        Some(kind) if kind == "create" || kind == "delete"
    )
}

pub(crate) fn classify_project_run_path_change(
    path: &str,
    change_kind: Option<&str>,
) -> Option<ProjectRunPathChangeKind> {
    let normalized = normalize_project_run_change_path(path);
    if normalized.is_empty() {
        return None;
    }

    let file_name = normalized.rsplit('/').next().unwrap_or("");
    match file_name {
        "package.json"
        | "pnpm-lock.yaml"
        | "package-lock.json"
        | "yarn.lock"
        | "pom.xml"
        | "build.gradle"
        | "build.gradle.kts"
        | "settings.gradle"
        | "settings.gradle.kts"
        | "mvnw"
        | "gradlew"
        | "pyproject.toml"
        | "requirements.txt"
        | "go.mod"
        | "cargo.toml" => {
            return Some(ProjectRunPathChangeKind::Catalog);
        }
        "pnpm-workspace.yaml"
        | "turbo.json"
        | "vite.config.js"
        | "vite.config.cjs"
        | "vite.config.mjs"
        | "vite.config.ts"
        | "next.config.js"
        | "next.config.mjs"
        | "next.config.ts"
        | "tsconfig.json"
        | "maven.config"
        | "jvm.config"
        | "gradle.properties"
        | "pipfile"
        | "poetry.lock"
        | "pytest.ini"
        | ".python-version"
        | "go.work"
        | "cargo.lock"
        | "rust-toolchain"
        | "rust-toolchain.toml" => {
            return Some(ProjectRunPathChangeKind::Environment);
        }
        _ => {}
    }

    if (normalized == "main.py"
        || normalized.ends_with("/main.py")
        || normalized == "app.py"
        || normalized.ends_with("/app.py"))
        && is_create_or_delete_change(change_kind)
    {
        return Some(ProjectRunPathChangeKind::Catalog);
    }

    if (normalized == "main.go"
        || normalized.ends_with("/main.go")
        || ((normalized.starts_with("cmd/") || normalized.contains("/cmd/"))
            && normalized.ends_with(".go")))
        && is_create_or_delete_change(change_kind)
    {
        return Some(ProjectRunPathChangeKind::Catalog);
    }

    if (normalized == "src/main.rs"
        || normalized.ends_with("/src/main.rs")
        || ((normalized.starts_with("src/bin/") || normalized.contains("/src/bin/"))
            && normalized.ends_with(".rs")))
        && is_create_or_delete_change(change_kind)
    {
        return Some(ProjectRunPathChangeKind::Catalog);
    }

    if ((normalized.starts_with("src/main/java/") || normalized.contains("/src/main/java/"))
        && normalized.ends_with(".java"))
        && is_create_or_delete_change(change_kind)
    {
        return Some(ProjectRunPathChangeKind::Catalog);
    }

    if (normalized.starts_with(".cargo/") || normalized.contains("/.cargo/"))
        && (normalized.ends_with("/config") || normalized.ends_with("/config.toml"))
    {
        return Some(ProjectRunPathChangeKind::Environment);
    }

    None
}
