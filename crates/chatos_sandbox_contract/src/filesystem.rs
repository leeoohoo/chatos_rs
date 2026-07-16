// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Component, Path, PathBuf};

pub fn filesystem_roots_for_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Vec<PathBuf> {
    let mut roots = paths
        .into_iter()
        .filter_map(|path| {
            let mut root = PathBuf::new();
            for component in path.components() {
                match component {
                    Component::Prefix(prefix) => root.push(prefix.as_os_str()),
                    Component::RootDir => {
                        root.push(Path::new(std::path::MAIN_SEPARATOR_STR));
                        return root.is_absolute().then_some(root);
                    }
                    Component::CurDir => {}
                    Component::ParentDir | Component::Normal(_) => return None,
                }
            }
            None
        })
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filesystem_roots_follow_platform_absolute_path_semantics() {
        let current = std::env::current_dir().expect("current directory");
        let roots = filesystem_roots_for_paths([current.as_path()]);
        assert_eq!(roots.len(), 1);
        assert!(roots[0].is_absolute());
        assert!(current.starts_with(roots[0].as_path()));
    }
}
