// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) fn is_completed_project_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "done" | "completed" | "succeeded" | "success"
    )
}

pub(crate) fn canonical_project_status(status: &str) -> String {
    if is_completed_project_status(status) {
        "done".to_string()
    } else {
        status.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{canonical_project_status, is_completed_project_status};

    #[test]
    fn normalizes_all_legacy_completion_aliases_to_done() {
        for status in ["done", "completed", "succeeded", "success", " COMPLETED "] {
            assert!(is_completed_project_status(status));
            assert_eq!(canonical_project_status(status), "done");
        }
        assert!(!is_completed_project_status("in_progress"));
        assert_eq!(canonical_project_status(" in_progress "), "in_progress");
    }
}
