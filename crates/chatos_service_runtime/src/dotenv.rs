// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

pub fn service_dotenv_files(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut current = Some(manifest_dir);
    for _ in 0..3 {
        let Some(directory) = current else {
            break;
        };
        let path = directory.join(".env");
        if !files.iter().any(|existing| existing == &path) {
            files.push(path);
        }
        current = directory.parent();
    }
    files
}

pub fn load_service_dotenv(manifest_dir: &Path) {
    for path in service_dotenv_files(manifest_dir) {
        let _ = dotenvy::from_path(path);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::service_dotenv_files;

    #[test]
    fn discovers_backend_service_and_repository_env_files_in_order() {
        let manifest_dir = Path::new("workspace").join("service").join("backend");
        let files = service_dotenv_files(manifest_dir.as_path());

        assert_eq!(
            files,
            vec![
                manifest_dir.join(".env"),
                Path::new("workspace").join("service").join(".env"),
                Path::new("workspace").join(".env"),
            ]
        );
    }
}
