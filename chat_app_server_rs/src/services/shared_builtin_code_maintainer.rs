// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use chatos_builtin_tools::CodeMaintainerHooks;

pub(crate) struct ChatosCodeMaintainerHooks;

impl CodeMaintainerHooks for ChatosCodeMaintainerHooks {
    fn note_workspace_path_changed(&self, path: &str) {
        crate::services::workspace_realtime_watcher::suppress_logged_path(path);
        crate::services::workspace_realtime_watcher::note_workspace_path_changed(path);
        crate::services::code_nav::symbol_index::invalidate_project_symbol_indexes_for_path(
            Path::new(path),
        );
    }
}
