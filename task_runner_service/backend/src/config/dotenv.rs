use std::path::{Path, PathBuf};

pub fn load_task_runner_dotenv() {
    for path in task_runner_dotenv_files() {
        let _ = dotenvy::from_path(path);
    }
}

pub(super) fn task_runner_dotenv_files() -> Vec<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();

    for path in [
        Some(manifest_dir.join(".env")),
        manifest_dir.parent().map(|path| path.join(".env")),
        manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.join(".env")),
    ]
    .into_iter()
    .flatten()
    {
        if !files.iter().any(|existing| existing == &path) {
            files.push(path);
        }
    }

    files
}
